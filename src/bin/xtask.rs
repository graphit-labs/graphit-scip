use std::{
    fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{SystemTime, UNIX_EPOCH},
};

#[path = "xtask/images.rs"]
mod images;

fn run(program: &str, args: &[&str]) -> Result<(), String> {
    let status = Command::new(program)
        .args(args)
        .status()
        .map_err(|_| format!("cannot launch {program}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} check failed"))
    }
}
fn check() -> Result<(), String> {
    run("cargo", &["fmt", "--all", "--", "--check"])?;
    run(
        "cargo",
        &[
            "clippy",
            "--locked",
            "--all-targets",
            "--",
            "-D",
            "warnings",
        ],
    )?;
    run("cargo", &["test", "--locked", "--all-targets"])
}
struct Scratch(PathBuf);
impl Scratch {
    fn new() -> Result<Self, String> {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "clock")?
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("graphit-scip-check-{}-{stamp}", std::process::id()));
        fs::create_dir(&path).map_err(|_| "cannot create check directory")?;
        Ok(Self(path))
    }
}
impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}
fn fixture_identity(path: &Path) -> Result<(String, String), String> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let metadata = fs::metadata(path).map_err(|_| "fixture identity")?;
        Ok((metadata.uid().to_string(), metadata.gid().to_string()))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        Ok(("0".into(), "0".into()))
    }
}
fn docker_smoke() -> Result<(), String> {
    run(
        "docker",
        &[
            "build",
            "--platform",
            "linux/amd64",
            "-t",
            "graphit-scip-clang:rust-check",
            "-f",
            "images/clang/Dockerfile",
            ".",
        ],
    )?;
    let scratch = Scratch::new()?;
    let source = scratch.0.join("source");
    let cache = scratch.0.join("cache");
    let output = scratch.0.join("output");
    for path in [&source.join("src"), &cache, &output] {
        fs::create_dir_all(path).map_err(|_| "cannot create smoke fixture")?;
    }
    let code = b"int graphit_keep(void) { return 42; }\n";
    fs::write(source.join("src/Keep.c"), code).map_err(|_| "fixture write")?;
    fs::write(
        source.join("src/Skip.c"),
        b"#error THIS_EXCLUDED_SOURCE_MUST_NOT_COMPILE\n",
    )
    .map_err(|_| "fixture write")?;
    let compdb=br#"[{"directory":"/synthetic/host","file":"src/Keep.c","arguments":["clang","-c","/synthetic/host/src/Keep.c"]},{"directory":"/synthetic/host","file":"src/Skip.c","arguments":["clang","-c","/synthetic/host/src/Skip.c"]}]"#;
    fs::write(source.join("compile_commands.json"), compdb).map_err(|_| "fixture write")?;
    fs::write(
        cache.join("graphit-allowed-files.json"),
        br#"["src/Keep.c"]"#,
    )
    .map_err(|_| "fixture write")?;
    let index_fixture = |root: &str, cache: &Path, output: &Path, overlay: bool| {
        let (uid, gid) = fixture_identity(&source)?;
        let overlay_root = scratch.0.join("overlay");
        fs::create_dir_all(&overlay_root).map_err(|_| "overlay fixture directory")?;
        let mut command = Command::new("docker");
        command
            .args(["run", "--rm", "--platform", "linux/amd64", "--mount"])
            .arg(format!(
                "type=bind,src={},dst={},readonly",
                source.display(),
                if overlay { "/source" } else { "/workspace" }
            ))
            .arg("--mount")
            .arg(format!("type=bind,src={},dst=/cache", cache.display()))
            .arg("--mount")
            .arg(format!("type=bind,src={},dst=/output", output.display()))
            .args([
                "-e",
                &format!("GRAPHIT_SCIP_UID={uid}"),
                "-e",
                &format!("GRAPHIT_SCIP_GID={gid}"),
                "-e",
                &format!("GRAPHIT_SCIP_HOST_ROOT={root}"),
            ]);
        if overlay {
            command
                .args([
                    "--cap-add",
                    "SYS_ADMIN",
                    "--security-opt",
                    "apparmor=unconfined",
                    "--mount",
                ])
                .arg(format!(
                    "type=bind,src={},dst=/overlay",
                    overlay_root.display()
                ))
                .args(["-e", "GRAPHIT_SCIP_OVERLAY=1"]);
        }
        let status = command
            .arg("graphit-scip-clang:rust-check")
            .status()
            .map_err(|_| "index Docker run")?;
        if !status.success() {
            return Err(format!("index Docker run: {status}"));
        }
        Ok(())
    };
    index_fixture("/synthetic/host", &cache, &output, true)?;
    let index = fs::read(output.join("index.scip")).map_err(|_| "SCIP output missing")?;
    if index.is_empty()
        || !index
            .windows(b"src/Keep.c".len())
            .any(|s| s == b"src/Keep.c")
    {
        return Err("SCIP output does not contain selected source".into());
    }
    let transformed = fs::read(cache.join("graphit-compile-commands.json"))
        .map_err(|_| "derived database missing")?;
    if !transformed
        .windows(b"/workspace/src/Keep.c".len())
        .any(|s| s == b"/workspace/src/Keep.c")
        || transformed.windows(b"Skip.c".len()).any(|s| s == b"Skip.c")
    {
        return Err("derived database selection failed".into());
    }
    if fs::read(source.join("src/Keep.c")).map_err(|_| "source read")? != code
        || fs::read(source.join("compile_commands.json")).map_err(|_| "source read")? != compdb
    {
        return Err("source changed".into());
    }
    let unicode_code = b"int graphit_unicode(void) { return 42; }\n";
    let unicode_compdb =
        r#"[{"directory":"C:\\Work","file":"STRASSE.cpp","command":"clang -c straße.cpp"}]"#;
    let unicode_cache = scratch.0.join("unicode-cache");
    let unicode_output = scratch.0.join("unicode-output");
    fs::create_dir(&unicode_cache).map_err(|_| "fixture directory")?;
    fs::create_dir(&unicode_output).map_err(|_| "fixture directory")?;
    fs::write(source.join("Straße.cpp"), unicode_code).map_err(|_| "fixture write")?;
    fs::write(source.join("compile_commands.json"), unicode_compdb).map_err(|_| "fixture write")?;
    fs::write(
        unicode_cache.join("graphit-allowed-files.json"),
        r#"["Straße.cpp"]"#,
    )
    .map_err(|_| "fixture write")?;
    index_fixture(r"C:\Work", &unicode_cache, &unicode_output, false)?;
    let index = fs::read(unicode_output.join("index.scip")).map_err(|_| "SCIP output missing")?;
    let name = "Straße.cpp".as_bytes();
    if !index.windows(name.len()).any(|bytes| bytes == name) {
        return Err("SCIP output does not contain canonical Unicode source".into());
    }
    if fs::read(source.join("Straße.cpp")).map_err(|_| "source read")? != unicode_code
        || fs::read(source.join("compile_commands.json")).map_err(|_| "source read")?
            != unicode_compdb.as_bytes()
    {
        return Err("Unicode source changed".into());
    }
    println!(
        "clang Docker smoke passed: Rust filter, selected SCIP documents, Unicode Windows command casefold, excluded invalid source and immutable inputs"
    );
    entrypoint_errors(&scratch.0, &source)?;
    dispatcher_image_protocol(&scratch.0, &source)?;
    Ok(())
}

fn entrypoint_errors(scratch: &Path, source: &Path) -> Result<(), String> {
    let overlay = scratch.join("dirty-overlay");
    fs::create_dir_all(overlay.join("upper")).map_err(|_| "overlay fixture")?;
    fs::write(overlay.join("upper/retained"), "untouched").map_err(|_| "overlay fixture")?;
    for (uid, gid, overlay_mode, expected) in [
        ("invalid", "1000", false, "GRAPHIT_SCIP_UID"),
        ("1000", "-1", false, "GRAPHIT_SCIP_GID"),
        ("1000", "1000", true, "overlay directory must be empty"),
    ] {
        let output = Command::new("docker")
            .args(["run", "--rm", "--platform", "linux/amd64", "--mount"])
            .arg(format!(
                "type=bind,src={},dst=/source,readonly",
                source.display()
            ))
            .arg("--mount")
            .arg(format!("type=bind,src={},dst=/overlay", overlay.display()))
            .args([
                "-e",
                &format!("GRAPHIT_SCIP_UID={uid}"),
                "-e",
                &format!("GRAPHIT_SCIP_GID={gid}"),
                "-e",
                if overlay_mode {
                    "GRAPHIT_SCIP_OVERLAY=1"
                } else {
                    "GRAPHIT_SCIP_OVERLAY=0"
                },
                "graphit-scip-clang:rust-check",
            ])
            .output()
            .map_err(|_| "entrypoint error Docker run")?;
        if output.status.code() != Some(1)
            || !String::from_utf8_lossy(&output.stderr).contains(expected)
        {
            return Err(format!(
                "entrypoint error case failed: {expected}: {}",
                String::from_utf8_lossy(&output.stderr)
            ));
        }
    }
    if fs::read(overlay.join("upper/retained")).map_err(|_| "overlay sentinel")? != b"untouched" {
        return Err("entrypoint changed occupied overlay".into());
    }
    println!("Rust entrypoint validation passed: UID/GID and occupied overlay errors");
    Ok(())
}

fn fixture_tools(directory: &Path) -> Result<(), String> {
    fs::create_dir_all(directory).map_err(|_| "fixture tools directory")?;
    let binary = directory.join("fixture-tool");
    let status = Command::new("rustc")
        .args([
            "--edition=2024",
            "-O",
            "tests/fixtures/dispatch/fake-tool.rs",
            "-o",
        ])
        .arg(&binary)
        .status()
        .map_err(|_| "fixture compiler")?;
    if !status.success() {
        return Err("fixture compilation failed".into());
    }
    fs::write(directory.join(".fixture-root"), "").map_err(|_| "fixture marker")?;
    for tool in [
        "scip-go",
        "scip-typescript",
        "python3",
        "pip",
        "scip-python",
        "scip-java",
        "cmake",
        "scip-clang",
        "dotnet",
        "scip-dotnet",
        "rust-analyzer",
        "bundle",
        "ruby",
        "dart",
        "php",
        "graphit-filter-clang-compdb",
    ] {
        fs::copy(&binary, directory.join(tool)).map_err(|_| "fixture tool copy")?;
    }
    Ok(())
}
fn dispatcher_image_protocol(scratch: &Path, source: &Path) -> Result<(), String> {
    let (uid, gid) = fixture_identity(source)?;
    for (mode, expected) in [
        ("", 0),
        ("output=empty", 1),
        ("output=missing", 1),
        ("exit=37", 37),
        ("sleep=10000", 124),
    ] {
        let cache = scratch.join(format!("protocol-cache-{expected}-{mode}"));
        let output = scratch.join(format!("protocol-output-{expected}-{mode}"));
        fs::create_dir_all(&output).map_err(|_| "protocol output directory")?;
        fixture_tools(&cache.join("bin"))?;
        fs::write(cache.join("bin/scip-go.mode"), mode).map_err(|_| "protocol fixture")?;
        fs::write(output.join("index.scip"), "stale").map_err(|_| "protocol fixture")?;
        let result = Command::new("docker")
            .args(["run", "--rm", "--platform", "linux/amd64", "--mount"])
            .arg(format!(
                "type=bind,src={},dst=/workspace,readonly",
                source.display()
            ))
            .arg("--mount")
            .arg(format!("type=bind,src={},dst=/cache", cache.display()))
            .arg("--mount")
            .arg(format!("type=bind,src={},dst=/output", output.display()))
            .args([
                "-e",
                &format!("GRAPHIT_SCIP_UID={uid}"),
                "-e",
                &format!("GRAPHIT_SCIP_GID={gid}"),
                "-e",
                "GRAPHIT_SCIP_FAMILY=go",
                "-e",
                "PATH=/cache/bin:/usr/local/bin:/usr/bin:/bin",
                "-e",
                if expected == 124 {
                    "GRAPHIT_SCIP_TIMEOUT_MS=50"
                } else {
                    "GRAPHIT_SCIP_TIMEOUT_MS=30000"
                },
                "graphit-scip-clang:rust-check",
            ])
            .status()
            .map_err(|_| "protocol Docker run")?;
        if result.code() != Some(expected) {
            return Err(format!(
                "dispatcher output protocol returned unexpected status {result}"
            ));
        }
        if expected == 0 {
            let bytes = fs::read(output.join("index.scip")).map_err(|_| "protocol output read")?;
            let index = <scip::types::Index as protobuf::Message>::parse_from_bytes(&bytes)
                .map_err(|_| "dispatcher protocol output is not SCIP")?;
            if index.documents.len() != 1 || index.documents[0].relative_path != "fixture.x" {
                return Err("dispatcher protocol output mismatch".into());
            }
        }
    }
    println!(
        "clang entrypoint native dispatcher protocol passed: success, stale/empty/missing output, upstream numeric failure, whole deadline"
    );
    dispatcher_signal(scratch, source)?;
    Ok(())
}

struct ContainerFixture(String);
impl Drop for ContainerFixture {
    fn drop(&mut self) {
        let _ = Command::new("docker")
            .args(["rm", "-f", &self.0])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}
fn dispatcher_signal(scratch: &Path, source: &Path) -> Result<(), String> {
    let (uid, gid) = fixture_identity(source)?;
    let cache = scratch.join("signal-cache");
    let output = scratch.join("signal-output");
    fs::create_dir_all(&output).map_err(|_| "signal output")?;
    fixture_tools(&cache.join("bin"))?;
    fs::write(cache.join("bin/scip-go.mode"), "sleep=10000").map_err(|_| "signal fixture")?;
    let name = format!(
        "graphit-scip-signal-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "clock")?
            .as_nanos()
    );
    let fixture = ContainerFixture(name);
    let started = Command::new("docker")
        .args([
            "run",
            "-d",
            "--name",
            &fixture.0,
            "--platform",
            "linux/amd64",
            "--mount",
        ])
        .arg(format!(
            "type=bind,src={},dst=/workspace,readonly",
            source.display()
        ))
        .arg("--mount")
        .arg(format!("type=bind,src={},dst=/cache", cache.display()))
        .arg("--mount")
        .arg(format!("type=bind,src={},dst=/output", output.display()))
        .args([
            "-e",
            &format!("GRAPHIT_SCIP_UID={uid}"),
            "-e",
            &format!("GRAPHIT_SCIP_GID={gid}"),
            "-e",
            "GRAPHIT_SCIP_FAMILY=go",
            "-e",
            "PATH=/cache/bin:/usr/local/bin:/usr/bin:/bin",
            "graphit-scip-clang:rust-check",
        ])
        .stdout(Stdio::null())
        .status()
        .map_err(|_| "signal Docker")?;
    if !started.success() {
        return Err("signal container failed to start".into());
    }
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while !cache.join("bin/calls.jsonl").is_file() {
        if std::time::Instant::now() >= deadline {
            return Err("signal child not started".into());
        }
        std::thread::sleep(std::time::Duration::from_millis(10));
    }
    let pid = Command::new("docker")
        .args(["exec", &fixture.0, "cat", "/proc/1/cmdline"])
        .output()
        .map_err(|_| "dispatcher PID inspection")?;
    if !pid.status.success() || !pid.stdout.starts_with(b"/usr/local/bin/graphit-scip-run\0") {
        return Err("dispatcher is not container PID 1".into());
    }
    let kill = Command::new("docker")
        .args(["kill", "--signal", "SIGTERM", &fixture.0])
        .stdout(Stdio::null())
        .status()
        .map_err(|_| "signal Docker kill")?;
    if !kill.success() {
        return Err("SIGTERM delivery failed".into());
    }
    let waited = Command::new("docker")
        .args(["wait", &fixture.0])
        .output()
        .map_err(|_| "signal wait")?;
    if !waited.status.success() || waited.stdout != b"130\n" {
        return Err("SIGTERM did not return cancellation status".into());
    }
    if output.join("index.scip").exists() {
        return Err("cancelled child produced accepted output".into());
    }
    println!(
        "clang entrypoint native dispatcher SIGTERM passed: exit130, owned child shutdown, no index; fixture container removed"
    );
    Ok(())
}

fn copy_tree(from: &Path, to: &Path) -> Result<(), String> {
    fs::create_dir_all(to).map_err(|_| "copy directory")?;
    for entry in fs::read_dir(from).map_err(|_| "read check sources")? {
        let entry = entry.map_err(|_| "read entry")?;
        let dest = to.join(entry.file_name());
        if entry.file_type().map_err(|_| "source type")?.is_dir() {
            copy_tree(&entry.path(), &dest)?;
        } else {
            fs::copy(entry.path(), dest).map_err(|_| "copy source")?;
        }
    }
    Ok(())
}
fn main() {
    let task = std::env::args().nth(1).unwrap_or_default();
    let result = match task.as_str() {
        "check" => check(),
        "docker-smoke" => docker_smoke(),
        "images-smoke" => images::run(std::env::args().nth(2).as_deref(), std::env::args().nth(3).as_deref()),
        "vendor-dotnet-smoke" => images::run(Some("dotnet"), None),
        "vendor-dotnet-corpus" => match std::env::args().nth(2) { Some(image) => images::run(Some("dotnet"), Some(&image)), None => Err("vendor-dotnet-corpus requires sha256:imageID".into()) },
        "gate" => check()
            .and_then(|_| run("cargo", &["audit", "--file", "Cargo.lock"]))
            .and_then(|_| docker_smoke()),
        _ => Err(
            "usage: cargo run --locked --bin xtask -- check|gate|docker-smoke|images-smoke [family [sha256:imageID]]|vendor-dotnet-smoke|vendor-dotnet-corpus <sha256:imageID>"
                .into(),
        ),
    };
    if let Err(error) = result {
        eprintln!("{error}");
        std::process::exit(1);
    }
}
