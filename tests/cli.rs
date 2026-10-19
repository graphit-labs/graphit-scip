use std::{
    fs,
    path::PathBuf,
    process::{Command, Output},
    sync::atomic::{AtomicU64, Ordering},
};
static COUNTER: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "graphit-compdb-test-{}-{}",
            std::process::id(),
            COUNTER.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    fn run(&self, input: &str, output: &str, extra: &[&str], root: bool) -> Output {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_filter-clang-compdb"));
        cmd.current_dir(&self.0);
        cmd.arg(self.0.join(input))
            .arg(self.0.join(output))
            .args(["--allowlist", self.0.join("allowed.json").to_str().unwrap()])
            .args(extra)
            .env_remove("GRAPHIT_SCIP_HOST_ROOT");
        if root {
            cmd.env("GRAPHIT_SCIP_HOST_ROOT", "/host/project");
        }
        cmd.output().unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}
#[test]
fn real_cli_preserves_input_and_never_executes_commands() {
    let f = Fixture::new();
    let source=br#"[{"directory":"/host/project","file":"src/a.c","command":"touch SHOULD_NEVER_EXIST","foreign":42},{"directory":"/elsewhere","file":"generated.c"}]"#;
    fs::write(f.0.join("in.json"), source).unwrap();
    fs::write(f.0.join("allowed.json"), br#"["src/a.c"]"#).unwrap();
    let result = f.run("in.json", "out.json", &[], true);
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    assert_eq!(fs::read(f.0.join("in.json")).unwrap(), source);
    assert!(!f.0.join("SHOULD_NEVER_EXIST").exists());
    assert_eq!(
        String::from_utf8(result.stderr).unwrap(),
        "Graphit: retained 1 unresolved compilation entries for final filtering\n"
    );
    let out: serde_json::Value =
        serde_json::from_slice(&fs::read(f.0.join("out.json")).unwrap()).unwrap();
    assert_eq!(out[0]["file"], "/workspace/src/a.c");
    assert_eq!(out[0]["foreign"], 42);
}
#[test]
fn real_cli_canonicalizes_expanded_windows_command_names() {
    let f = Fixture::new();
    let source = serde_json::to_vec(&serde_json::json!([{
        "directory": r"C:\Work",
        "file": "STRASSE.cpp",
        "command": "clang -c straße.cpp",
    }]))
    .unwrap();
    fs::write(f.0.join("in.json"), &source).unwrap();
    fs::write(f.0.join("allowed.json"), r#"["Straße.cpp"]"#).unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_filter-clang-compdb"))
        .current_dir(&f.0)
        .args(["in.json", "out.json", "--allowlist", "allowed.json"])
        .env("GRAPHIT_SCIP_HOST_ROOT", r"C:\Work")
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(fs::read(f.0.join("in.json")).unwrap(), source);
    let result: serde_json::Value =
        serde_json::from_slice(&fs::read(f.0.join("out.json")).unwrap()).unwrap();
    assert_eq!(result[0]["file"], "/workspace/Straße.cpp");
    assert_eq!(result[0]["command"], "clang -c Straße.cpp");
}
#[test]
fn parse_limits_environment_fail_before_output_is_touched() {
    let f = Fixture::new();
    fs::write(f.0.join("in.json"), b"[{}]").unwrap();
    for (args, root) in [
        (vec!["--max-input-bytes", "3"], true),
        (vec!["--max-output-bytes", "1"], true),
        (vec!["--max-entries", "0"], true),
        (vec!["--unknown", "42"], true),
        (vec![], false),
    ] {
        fs::write(f.0.join("out.json"), b"sentinel").unwrap();
        let output = f.run("in.json", "out.json", &args, root);
        assert!(!output.status.success());
        assert_eq!(fs::read(f.0.join("out.json")).unwrap(), b"sentinel");
    }
    fs::write(f.0.join("in.json"), b"[invalid-sensitive-command]").unwrap();
    let output = f.run("in.json", "out.json", &[], true);
    assert!(!output.status.success());
    assert!(!String::from_utf8_lossy(&output.stderr).contains("sensitive"));
    assert_eq!(fs::read(f.0.join("out.json")).unwrap(), b"sentinel");
}
#[test]
fn aliases_including_hardlinks_are_rejected() {
    let f = Fixture::new();
    let bytes = b"[{}]";
    fs::write(f.0.join("in.json"), bytes).unwrap();
    assert!(!f.run("in.json", "in.json", &[], true).status.success());
    fs::hard_link(f.0.join("in.json"), f.0.join("alias.json")).unwrap();
    assert!(!f.run("in.json", "alias.json", &[], true).status.success());
    assert_eq!(fs::read(f.0.join("in.json")).unwrap(), bytes);
    #[cfg(unix)]
    {
        std::os::unix::fs::symlink(f.0.join("in.json"), f.0.join("symlink.json")).unwrap();
        assert!(!f.run("in.json", "symlink.json", &[], true).status.success());
    }
}
#[test]
fn cli_exact_bound_and_missing_allowlist() {
    let f = Fixture::new();
    fs::write(f.0.join("in.json"), b"[{}]").unwrap();
    let output = f.run(
        "in.json",
        "out.json",
        &[
            "--max-input-bytes",
            "4",
            "--max-output-bytes",
            "4",
            "--max-entries",
            "1",
        ],
        true,
    );
    assert!(output.status.success());
    assert_eq!(fs::read(f.0.join("out.json")).unwrap(), b"[{}]");
    fs::write(f.0.join("allowed.json"), b"[1]").unwrap();
    assert!(!f.run("in.json", "out.json", &[], true).status.success());
    fs::remove_file(f.0.join("allowed.json")).unwrap();
    for root_bytes in [32767, 32768, 32769] {
        let root = format!("/{}", "a".repeat(root_bytes - 1));
        fs::write(f.0.join("out.json"), b"sentinel").unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_filter-clang-compdb"))
            .current_dir(&f.0)
            .args([
                "in.json",
                "out.json",
                "--allowlist",
                "absent.json",
                "--max-input-bytes",
                "4",
            ])
            .env("GRAPHIT_SCIP_HOST_ROOT", root)
            .output()
            .unwrap();
        assert_eq!(output.status.success(), root_bytes <= 32768);
        assert_eq!(fs::read(f.0.join("in.json")).unwrap(), b"[{}]");
        assert_eq!(
            fs::read(f.0.join("out.json")).unwrap(),
            if root_bytes <= 32768 {
                b"[{}]".as_slice()
            } else {
                b"sentinel"
            }
        );
    }
}
