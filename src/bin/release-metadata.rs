use graphit_scip_tools::release::{self, Error, MAX_OUTPUT_BYTES};
use std::{io::Write, path::Path};

fn run() -> Result<(), Error> {
    if std::env::args_os().len() != 1 {
        return Err(Error::Tag);
    }
    let tag = std::env::var("RELEASE_TAG").map_err(|_| Error::Tag)?;
    let images = Path::new("images");
    let metadata = release::prepare(&tag, images)?;
    let json = metadata.json(MAX_OUTPUT_BYTES)?;
    if let Some(output) = std::env::var_os("GITHUB_OUTPUT") {
        metadata.append(images, Path::new(&output))?;
    }
    let mut stdout = std::io::stdout().lock();
    stdout
        .write_all(&json)
        .and_then(|_| stdout.write_all(b"\n"))
        .map_err(|_| Error::Io)
}
fn main() {
    if let Err(error) = run() {
        eprintln!("Graphit: {error}");
        std::process::exit(2);
    }
}
