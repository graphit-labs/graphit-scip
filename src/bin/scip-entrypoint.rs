#[cfg(target_os = "linux")]
use std::{ffi::OsStr, path::Path};

#[cfg(any(target_os = "linux", test))]
fn identity(name: &str, value: &str) -> Result<u32, String> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return Err(format!("{name} must contain a numeric UID/GID"));
    }
    value
        .parse::<u32>()
        .ok()
        .filter(|id| *id != u32::MAX)
        .ok_or_else(|| format!("{name} is outside the supported UID/GID range"))
}

#[cfg(target_os = "linux")]
fn ownership(
    mode: Option<&OsStr>,
    uid: Option<&OsStr>,
    gid: Option<&OsStr>,
    output: &Path,
) -> Result<(u32, u32), String> {
    use std::os::unix::fs::MetadataExt;

    if let Some(mode) = mode {
        if mode != "output-mount" {
            return Err("GRAPHIT_SCIP_OWNER must be output-mount when present".into());
        }
        if uid.is_some() || gid.is_some() {
            return Err("GRAPHIT_SCIP_OWNER is mutually exclusive with GRAPHIT_SCIP_UID and GRAPHIT_SCIP_GID".into());
        }
        let metadata = std::fs::metadata(output)
            .map_err(|error| format!("read output mount {}: {error}", output.display()))?;
        if !metadata.is_dir() {
            return Err(format!(
                "output mount must be a directory: {}",
                output.display()
            ));
        }
        return Ok((
            identity("output mount UID", &metadata.uid().to_string())?,
            identity("output mount GID", &metadata.gid().to_string())?,
        ));
    }
    let required = |name: &str, value: Option<&OsStr>| {
        let value = value.ok_or_else(|| format!("{name} is required"))?;
        identity(
            name,
            value
                .to_str()
                .ok_or_else(|| format!("{name} must contain a numeric UID/GID"))?,
        )
    };
    Ok((
        required("GRAPHIT_SCIP_UID", uid)?,
        required("GRAPHIT_SCIP_GID", gid)?,
    ))
}

#[cfg(target_os = "linux")]
fn empty_directory(path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(path).map_err(|e| format!("create {}: {e}", path.display()))?;
    if std::fs::read_dir(path)
        .map_err(|e| format!("read {}: {e}", path.display()))?
        .next()
        .transpose()
        .map_err(|e| format!("read {}: {e}", path.display()))?
        .is_some()
    {
        return Err(format!(
            "overlay directory must be empty: {}",
            path.display()
        ));
    }
    Ok(())
}

#[cfg(target_os = "linux")]
fn run() -> Result<(), String> {
    use std::{env, fs, os::unix::process::CommandExt, process::Command};

    let (uid, gid) = ownership(
        env::var_os("GRAPHIT_SCIP_OWNER").as_deref(),
        env::var_os("GRAPHIT_SCIP_UID").as_deref(),
        env::var_os("GRAPHIT_SCIP_GID").as_deref(),
        Path::new("/output"),
    )?;
    if env::var_os("GRAPHIT_SCIP_OVERLAY").is_some_and(|v| v == "1") {
        if !Path::new("/source").is_dir() || !Path::new("/overlay").is_dir() {
            return Err("overlay mode requires /source and /overlay directories".into());
        }
        for directory in ["/overlay/upper", "/overlay/work"] {
            let path = Path::new(directory);
            empty_directory(path)?;
            std::os::unix::fs::chown(path, Some(uid), Some(gid))
                .map_err(|e| format!("set ownership of {directory}: {e}"))?;
        }
        fs::create_dir_all("/workspace").map_err(|e| format!("create /workspace: {e}"))?;
        let status = Command::new("mount")
            .args([
                "-t",
                "overlay",
                "overlay",
                "-o",
                "lowerdir=/source,upperdir=/overlay/upper,workdir=/overlay/work",
                "/workspace",
            ])
            .status()
            .map_err(|e| format!("mount overlay: {e}"))?;
        if !status.success() {
            return Err(format!("mount overlay failed: {status}"));
        }
    }
    for directory in [
        "/cache/home",
        "/cache/tmp",
        "/cache/xdg-cache",
        "/cache/xdg-config",
        "/output",
    ] {
        fs::create_dir_all(directory).map_err(|e| format!("create {directory}: {e}"))?;
        // Docker Desktop bind mounts can reject chown while mapping the configured identity.
        let _ = std::os::unix::fs::chown(directory, Some(uid), Some(gid));
    }
    let error = Command::new("setpriv")
        .args(["--no-new-privs", "--bounding-set=-all", "--reuid"])
        .arg(uid.to_string())
        .arg("--regid")
        .arg(gid.to_string())
        .args(["--clear-groups", "/usr/local/bin/graphit-scip-run"])
        .args(env::args_os().skip(1))
        .env("HOME", "/cache/home")
        .env("TMPDIR", "/cache/tmp")
        .env("XDG_CACHE_HOME", "/cache/xdg-cache")
        .env("XDG_CONFIG_HOME", "/cache/xdg-config")
        .current_dir("/workspace")
        .exec();
    Err(format!("execute dispatcher: {error}"))
}

#[cfg(not(target_os = "linux"))]
fn run() -> Result<(), String> {
    Err("the container entrypoint requires Linux".into())
}

fn main() {
    if let Err(error) = run() {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_non_numeric_and_reserved_identity_before_effects() {
        for value in ["", "-1", "+1", "1.0", "١", "4294967296", "4294967295"] {
            assert!(identity("GRAPHIT_SCIP_UID", value).is_err(), "{value}");
        }
    }
    #[test]
    fn accepts_root_and_numeric_identity() {
        assert_eq!(identity("GRAPHIT_SCIP_UID", "0").unwrap(), 0);
        assert_eq!(identity("GRAPHIT_SCIP_GID", "1000").unwrap(), 1000);
        assert_eq!(
            identity("GRAPHIT_SCIP_UID", "4294967294").unwrap(),
            u32::MAX - 1
        );
    }

    #[cfg(target_os = "linux")]
    mod owner {
        use super::*;
        use std::{
            fs,
            os::unix::{ffi::OsStrExt, fs::MetadataExt},
            path::PathBuf,
            sync::atomic::{AtomicUsize, Ordering},
        };

        struct Directory(PathBuf);
        impl Directory {
            fn new() -> Self {
                static NEXT: AtomicUsize = AtomicUsize::new(0);
                let path = std::env::temp_dir().join(format!(
                    "graphit-scip-entrypoint-owner-{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, Ordering::Relaxed)
                ));
                fs::create_dir(&path).unwrap();
                Self(path)
            }
        }
        impl Drop for Directory {
            fn drop(&mut self) {
                fs::remove_dir_all(&self.0).unwrap();
            }
        }

        #[test]
        fn explicit_pair_preserves_identity_without_observing_output() {
            let root = Directory::new();
            let missing = root.0.join("missing");
            assert_eq!(
                ownership(
                    None,
                    Some(OsStr::new("0")),
                    Some(OsStr::new("1000")),
                    &missing
                )
                .unwrap(),
                (0, 1000)
            );
            for (uid, gid) in [
                (None, None),
                (Some("1000"), None),
                (None, Some("1000")),
                (Some(""), Some("1000")),
                (Some("1000"), Some("-1")),
                (Some("4294967295"), Some("1000")),
            ] {
                assert!(
                    ownership(None, uid.map(OsStr::new), gid.map(OsStr::new), &missing).is_err()
                );
            }
            assert!(!missing.exists());
        }

        #[test]
        fn rejects_unknown_and_non_unicode_modes() {
            let missing = Path::new("/unused-output-owner-fixture");
            for mode in [
                OsStr::new(""),
                OsStr::new("auto"),
                OsStr::from_bytes(b"\xff"),
            ] {
                let error = ownership(Some(mode), None, None, missing).unwrap_err();
                assert!(error.contains("GRAPHIT_SCIP_OWNER must be output-mount"));
            }
        }

        #[test]
        fn mount_mode_rejects_any_explicit_identity_before_metadata() {
            let missing = Path::new("/unused-output-owner-fixture");
            for (uid, gid) in [(Some("0"), None), (None, Some("")), (Some("0"), Some("0"))] {
                let error = ownership(
                    Some(OsStr::new("output-mount")),
                    uid.map(OsStr::new),
                    gid.map(OsStr::new),
                    missing,
                )
                .unwrap_err();
                assert!(error.contains("mutually exclusive"));
            }
        }

        #[test]
        fn mount_mode_observes_real_linux_directory_without_modification() {
            let root = Directory::new();
            let sentinel = root.0.join("retained");
            fs::write(&sentinel, b"unchanged").unwrap();
            let before = fs::metadata(&root.0).unwrap();
            let owner = ownership(Some(OsStr::new("output-mount")), None, None, &root.0).unwrap();
            assert_eq!(owner, (before.uid(), before.gid()));
            let after = fs::metadata(&root.0).unwrap();
            assert_eq!(
                (after.uid(), after.gid(), after.mode()),
                (before.uid(), before.gid(), before.mode())
            );
            assert_eq!(fs::read(sentinel).unwrap(), b"unchanged");
        }

        #[test]
        fn mount_mode_requires_an_existing_directory() {
            let root = Directory::new();
            let missing = root.0.join("missing");
            assert!(ownership(Some(OsStr::new("output-mount")), None, None, &missing).is_err());
            assert!(!missing.exists());
            let file = root.0.join("file");
            fs::write(&file, b"retained").unwrap();
            let error = ownership(Some(OsStr::new("output-mount")), None, None, &file).unwrap_err();
            assert!(error.contains("must be a directory"));
            assert_eq!(fs::read(file).unwrap(), b"retained");
        }
    }
}
