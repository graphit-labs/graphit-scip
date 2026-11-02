use super::{Control, Error, MAX_CAPTURE_BYTES};
use process_wrap::std::{ChildWrapper, CommandWrap};
use std::{
    io::Read,
    process::{Command, ExitStatus, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

const POLL: Duration = Duration::from_millis(20);
const FAST_POLL: Duration = Duration::from_millis(1);
const DRAIN: Duration = Duration::from_secs(1);

struct OwnedChild {
    inner: Box<dyn ChildWrapper>,
    stopped: bool,
}
impl OwnedChild {
    fn stop(&mut self) -> Result<(), Error> {
        if self.stopped {
            return Ok(());
        }
        let terminated = self.inner.start_kill();
        // Darwin can return EPERM when a process group contains only its
        // unreaped leader (XNU killpg1 excludes zombies). Reap that child and
        // retry the group signal: only ESRCH is accepted as an empty group;
        // a real permission failure with surviving members remains an error.
        #[cfg(target_os = "macos")]
        let terminated = if terminated
            .as_ref()
            .is_err_and(|error| error.raw_os_error() == Some(nix::errno::Errno::EPERM as i32))
            && self
                .inner
                .try_wait()
                .map_err(|error| io_error("reap exited process", &error))?
                .is_some()
        {
            self.inner.start_kill()
        } else {
            terminated
        };
        if let Err(error) = terminated {
            #[cfg(unix)]
            if error.raw_os_error() != Some(nix::errno::Errno::ESRCH as i32) {
                return Err(io_error("terminate process group", &error));
            }
            #[cfg(not(unix))]
            return Err(io_error("terminate process job", &error));
        }
        self.inner
            .wait()
            .map_err(|error| io_error("reap process", &error))?;
        self.stopped = true;
        Ok(())
    }
}
impl Drop for OwnedChild {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

// Report the failing operation and OS classification without exposing command,
// arguments, paths, environment or captured metadata.
fn io_error(operation: &str, error: &std::io::Error) -> Error {
    eprintln!(
        "SCIP process {operation} failed: {:?} (OS error {:?})",
        error.kind(),
        error.raw_os_error()
    );
    Error::Io
}

fn status(status: ExitStatus) -> Result<(), Error> {
    if status.success() {
        return Ok(());
    }
    #[cfg(unix)]
    {
        use std::os::unix::process::ExitStatusExt;
        Err(Error::Child(
            status
                .code()
                .unwrap_or_else(|| 128 + status.signal().unwrap_or(1)),
        ))
    }
    #[cfg(not(unix))]
    Err(Error::Child(status.code().unwrap_or(1)))
}

fn spawn(command: Command, capture: bool) -> Result<OwnedChild, Error> {
    let mut command = command;
    command.stdin(Stdio::inherit()).stderr(Stdio::inherit());
    command.stdout(if capture {
        Stdio::piped()
    } else {
        Stdio::inherit()
    });
    let mut wrapper = CommandWrap::from(command);
    #[cfg(unix)]
    wrapper.wrap(process_wrap::std::ProcessGroup::leader());
    #[cfg(windows)]
    wrapper.wrap(process_wrap::std::JobObject);
    wrapper
        .spawn()
        .map(|inner| OwnedChild {
            inner,
            stopped: false,
        })
        .map_err(|error| {
            // CreateProcess rejects command lines beyond its native UTF-16
            // bound even when each individual metadata field was admitted.
            #[cfg(windows)]
            if error.raw_os_error() == Some(206) {
                return Error::Limit;
            }
            match error.kind() {
                std::io::ErrorKind::NotFound => Error::MissingTool,
                std::io::ErrorKind::PermissionDenied => Error::DeniedTool,
                _ => io_error("spawn", &error),
            }
        })
}

pub(super) fn execute(
    command: Command,
    control: &Control,
    capture: bool,
) -> Result<Vec<u8>, Error> {
    control.check()?;
    let mut child = spawn(command, capture)?;
    let started = Instant::now();
    let reader = if capture {
        let mut stdout = child.inner.stdout().take().ok_or(Error::Io)?;
        let (sender, receiver) = mpsc::sync_channel(1);
        let thread = thread::Builder::new()
            .name("scip-metadata".into())
            .spawn(move || {
                let result = (|| {
                    let mut bytes = Vec::new();
                    bytes
                        .try_reserve_exact(MAX_CAPTURE_BYTES)
                        .map_err(|_| Error::Limit)?;
                    let mut chunk = [0u8; 4096];
                    loop {
                        let count = stdout
                            .read(&mut chunk)
                            .map_err(|error| io_error("read metadata", &error))?;
                        if count == 0 {
                            return Ok(bytes);
                        }
                        if bytes
                            .len()
                            .checked_add(count)
                            .is_none_or(|size| size > MAX_CAPTURE_BYTES)
                        {
                            return Err(Error::Limit);
                        }
                        bytes.extend_from_slice(&chunk[..count]);
                    }
                })();
                let _ = sender.send(result);
            })
            .map_err(|_| Error::Io)?;
        Some((thread, receiver))
    } else {
        None
    };
    let mut captured = None;
    let result = loop {
        if let Err(error) = control.check() {
            break Err(error);
        }
        if let Some((_, receiver)) = &reader {
            match receiver.try_recv() {
                Ok(value) => {
                    let error = value.as_ref().err().copied();
                    captured = Some(value);
                    if let Some(error) = error {
                        break Err(error);
                    }
                }
                Err(mpsc::TryRecvError::Disconnected) if captured.is_none() => {
                    break Err(Error::Io);
                }
                _ => {}
            }
        }
        // This runner installs exactly one Windows wrapper: JobObject. Poll
        // its direct child without consuming completion-port notifications;
        // JobObject::wait must still receive them during group cleanup.
        #[cfg(windows)]
        let exited = child.inner.inner_mut().try_wait();
        #[cfg(not(windows))]
        let exited = child.inner.try_wait();
        match exited {
            Ok(Some(exited)) => break status(exited),
            Ok(None) => thread::sleep(if started.elapsed() < Duration::from_secs(1) {
                FAST_POLL
            } else {
                POLL
            }),
            Err(error) => break Err(io_error("poll process", &error)),
        }
    };
    // Even a successful leader can leave a descendant holding stdout open.
    // Terminate the owned group before draining; wait() reaps our direct child.
    let stopped = child.stop();
    let capture_result = if let Some((reader, receiver)) = reader {
        let drain_until = Instant::now() + DRAIN;
        while !reader.is_finished() && Instant::now() < drain_until {
            thread::sleep(Duration::from_millis(1));
        }
        if !reader.is_finished() {
            Err(Error::Io)
        } else {
            reader.join().map_err(|_| Error::Io)?;
            match captured {
                Some(value) => value,
                None => receiver.try_recv().map_err(|_| Error::Io)?,
            }
        }
    } else {
        Ok(Vec::new())
    };
    stopped?;
    if matches!(result, Err(Error::Cancelled | Error::Deadline)) {
        return result.map(|_| Vec::new());
    }
    let bytes = capture_result?;
    result?;
    control.check()?;
    Ok(bytes)
}
