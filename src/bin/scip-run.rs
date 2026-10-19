use graphit_scip_tools::dispatch::{self, Control, Error, Settings};
use std::{sync::atomic::Ordering, time::Duration};

fn execute() -> Result<(), Error> {
    if std::env::args_os().len() != 1 {
        return Err(Error::Config);
    }
    let timeout = std::env::var_os("GRAPHIT_SCIP_TIMEOUT_MS")
        .map(|value| {
            let value = value.to_str().ok_or(Error::Config)?;
            if value.is_empty()
                || value.len() > 20
                || !value.bytes().all(|byte| byte.is_ascii_digit())
            {
                return Err(Error::Config);
            }
            let milliseconds: u64 = value.parse().map_err(|_| Error::Config)?;
            if milliseconds == 0 {
                return Err(Error::Config);
            }
            Ok(Duration::from_millis(milliseconds))
        })
        .transpose()?;
    let control = Control::with_timeout(timeout)?;
    let cancelled = control.cancelled.clone();
    ctrlc::set_handler(move || cancelled.store(true, Ordering::Relaxed)).map_err(|_| Error::Io)?;
    dispatch::run(&Settings::container()?, &control)
}
fn main() {
    if let Err(error) = execute() {
        eprintln!("{error}");
        std::process::exit(error.exit_code());
    }
}
