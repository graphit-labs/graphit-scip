use graphit_scip_tools::compdb::{self, BoundedBytes, Limits};
use same_file::Handle;
use std::{
    ffi::OsString,
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
};

fn read_bounded(path: &Path, limit: usize) -> Result<Vec<u8>, &'static str> {
    let mut file = File::open(path).map_err(|_| "cannot read input")?;
    let metadata = file.metadata().map_err(|_| "cannot inspect input")?;
    if !metadata.is_file() {
        return Err("input must be a regular file");
    }
    if metadata.len() > limit as u64 {
        return Err("input byte limit exceeded");
    }
    let mut output = BoundedBytes::new(limit);
    let mut buffer = [0u8; 8192];
    loop {
        let size = file.read(&mut buffer).map_err(|_| "cannot read input")?;
        if size == 0 {
            break;
        }
        output
            .write_all(&buffer[..size])
            .map_err(|_| "input byte limit exceeded")?;
    }
    Ok(output.into_vec())
}
fn check_alias(handle: &Handle, input: &Path, output: &Path) -> Result<(), &'static str> {
    if &Handle::from_path(input).map_err(|_| "cannot inspect input identity")? != handle {
        return Err("input identity changed");
    }
    match Handle::from_path(output) {
        Ok(output) if &output == handle => Err("input and output must be different files"),
        Ok(_) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(_) => Err("cannot inspect output identity"),
    }
}
fn run(args: impl IntoIterator<Item = OsString>) -> Result<(), String> {
    let mut args = args.into_iter();
    let input = PathBuf::from(
        args.next()
            .ok_or("usage: filter-clang-compdb INPUT OUTPUT [OPTIONS]")?,
    );
    let output = PathBuf::from(
        args.next()
            .ok_or("usage: filter-clang-compdb INPUT OUTPUT [OPTIONS]")?,
    );
    let mut allowlist = PathBuf::from("/cache/graphit-allowed-files.json");
    let mut limits = Limits::default();
    while let Some(flag) = args.next() {
        let flag = flag.to_str().ok_or("invalid option")?;
        let value = args.next().ok_or("option value required")?;
        if flag == "--allowlist" {
            allowlist = PathBuf::from(value);
            continue;
        }
        let number = value
            .to_str()
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|n| *n > 0)
            .ok_or("limits must be positive integers")?;
        match flag {
            "--max-input-bytes" => limits.input_bytes = number,
            "--max-allowlist-bytes" => limits.allowlist_bytes = number,
            "--max-entries" => limits.entries = number,
            "--max-output-bytes" => limits.output_bytes = number,
            _ => return Err("unknown option".into()),
        }
    }
    let root = std::env::var("GRAPHIT_SCIP_HOST_ROOT")
        .map_err(|_| "GRAPHIT_SCIP_HOST_ROOT is required")?;
    let identity = Handle::from_path(&input).map_err(|_| "cannot inspect input identity")?;
    check_alias(&identity, &input, &output)?;
    let bytes = read_bounded(&input, limits.input_bytes)?;
    let commands = compdb::parse_commands(&bytes, limits).map_err(|e| e.to_string())?;

    let allowed = match std::fs::metadata(&allowlist) {
        Ok(_) => {
            let bytes = read_bounded(&allowlist, limits.allowlist_bytes)?;
            Some(compdb::parse_allowlist(&bytes, limits).map_err(|e| e.to_string())?)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(_) => return Err("cannot inspect allowed-source manifest".into()),
    };
    let (commands, unresolved) =
        compdb::transform(commands, &root, allowed, limits).map_err(|e| e.to_string())?;
    let bytes = compdb::serialize(&commands, limits).map_err(|e| e.to_string())?;
    check_alias(&identity, &input, &output)?;
    let mut target = File::create(&output).map_err(|_| "cannot create output")?;
    target
        .write_all(&bytes)
        .map_err(|_| "cannot write output")?;
    if unresolved > 0 {
        eprintln!(
            "Graphit: retained {unresolved} unresolved compilation entries for final filtering"
        );
    }
    Ok(())
}
fn main() {
    if let Err(error) = run(std::env::args_os().skip(1)) {
        eprintln!("Graphit: {error}");
        std::process::exit(2);
    }
}
