use graphit_scip_tools::dispatch::{self, Control, Error, Family, Settings};
use protobuf::Message;
use serde_json::Value;
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        OnceLock,
        atomic::{AtomicU64, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
static FAKE: OnceLock<PathBuf> = OnceLock::new();
fn fake() -> &'static Path {
    FAKE.get_or_init(|| {
        let root =
            std::env::temp_dir().join(format!("graphit-dispatch-tools-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        let binary = root.join(format!("fake{}", std::env::consts::EXE_SUFFIX));
        assert!(
            Command::new("rustc")
                .args(["--edition=2024", "-O"])
                .arg(
                    Path::new(env!("CARGO_MANIFEST_DIR"))
                        .join("tests/fixtures/dispatch/fake-tool.rs")
                )
                .arg("-o")
                .arg(&binary)
                .status()
                .unwrap()
                .success()
        );
        binary
    })
}
struct Fixture {
    root: PathBuf,
    settings: Settings,
}
impl Fixture {
    fn new(family: Family) -> Self {
        let root = std::env::temp_dir().join(format!(
            "graphit-dispatch-{}-{}-ação '#{{literal}}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        // macOS temp_dir may use /var while subprocess cwd resolves /private/var.
        let root = fs::canonicalize(root).unwrap();
        for name in ["workspace", "cache", "output", "bin"] {
            fs::create_dir(root.join(name)).unwrap();
        }
        fs::write(root.join("bin/.fixture-root"), "").unwrap();
        let tools = [
            "scip-go",
            "scip-typescript",
            "python3",
            "pip",
            "scip-python",
            "scip-java",
            "cmake",
            "graphit-filter-clang-compdb",
            "scip-clang",
            "dotnet",
            "scip-dotnet",
            "rust-analyzer",
            "bundle",
            "ruby",
            "dart",
            "php",
        ];
        // Copy in a child: concurrent forks in this test process must not
        // inherit executable write descriptors (Linux ETXTBSY). Keep separate
        // files, since the production tool validator correctly rejects aliases.
        assert!(
            Command::new(fake())
                .arg("--fixture-install")
                .arg(root.join("bin"))
                .args(tools)
                .status()
                .unwrap()
                .success()
        );
        let settings = Settings {
            family,
            workspace: root.join("workspace"),
            cache: root.join("cache"),
            output: root.join("output/index.scip"),
            path: root.join("bin").into_os_string(),
            host_root: None,
            project_name: "fixture ação \"name\"".into(),
            php_autoload: root.join("autoload.php"),
        };
        Self { root, settings }
    }
    fn input(&self, name: &str, body: &str) {
        let path = self.settings.workspace.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, body).unwrap();
    }
    fn mode(&self, tool: &str, body: &str) {
        fs::write(self.root.join("bin").join(format!("{tool}.mode")), body).unwrap();
    }
    fn calls(&self) -> Vec<Value> {
        fs::read_to_string(self.root.join("bin/calls.jsonl"))
            .unwrap_or_default()
            .lines()
            .map(|line| serde_json::from_str(line).unwrap())
            .collect()
    }
    fn run(&self) -> Result<(), Error> {
        dispatch::run(&self.settings, &Control::default())
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
fn args(call: &Value) -> Vec<&str> {
    call["args"]
        .as_array()
        .unwrap()
        .iter()
        .map(|argument| argument.as_str().unwrap())
        .collect()
}

#[test]
fn all_ten_journeys_execute_real_tools_with_isolated_paths() {
    for (family, tool, expected) in [
        (Family::Go, "scip-go", vec!["index", "--output"]),
        (
            Family::TypeScript,
            "scip-typescript",
            vec!["index", "--output"],
        ),
        (Family::Python, "scip-python", vec!["index", "--cwd"]),
        (Family::Java, "scip-java", vec!["index"]),
        (Family::Clang, "scip-clang", vec![]),
        (Family::Dotnet, "scip-dotnet", vec!["index", "--output"]),
        (Family::Rust, "rust-analyzer", vec!["scip", ".", "--output"]),
        (
            Family::Ruby,
            "bundle",
            vec!["exec", "scip-ruby", "--gem-metadata"],
        ),
        (
            Family::Dart,
            "dart",
            vec!["pub", "global", "run", "scip_dart", "--output"],
        ),
        (Family::Php, "php", vec!["-r"]),
    ] {
        let f = Fixture::new(family);
        f.input("tsconfig.json", "{}");
        f.input("compile_commands.json", "[]");
        f.input("composer.json", "{}");
        f.input("composer.lock", "{}");
        f.input("fixture.csproj", "");
        fs::write(&f.settings.output, "stale").unwrap();
        f.run().unwrap();
        let index =
            scip::types::Index::parse_from_bytes(&fs::read(&f.settings.output).unwrap()).unwrap();
        assert_eq!(index.documents[0].relative_path, "fixture.x");
        assert_eq!(index.documents[0].position_encoding.value(), 1);
        let calls = f.calls();
        let last = calls.last().unwrap();
        assert_eq!(last["tool"], tool);
        assert!(args(last).starts_with(&expected));
        for call in calls {
            assert_eq!(call["cwd"], f.settings.workspace.to_str().unwrap());
            assert!(
                call["env"]["PATH"]
                    .as_str()
                    .unwrap()
                    .contains(f.root.to_str().unwrap())
            );
        }
    }
}

#[test]
fn typescript_priority_and_provided_or_generated_configuration() {
    for (files, flag) in [
        (
            vec!["tsconfig.json", "pnpm-lock.yaml", "yarn.lock"],
            Some("--pnpm-workspaces"),
        ),
        (
            vec!["tsconfig.json", "yarn.lock"],
            Some("--yarn-workspaces"),
        ),
        (vec!["tsconfig.json"], None),
    ] {
        let f = Fixture::new(Family::TypeScript);
        for file in files {
            f.input(file, "{}");
        }
        f.run().unwrap();
        let calls = f.calls();
        let arguments = args(&calls[0]);
        assert_eq!(arguments.get(1).copied(), flag.or(Some("--output")));
    }
    for provided in [false, true] {
        let f = Fixture::new(Family::TypeScript);
        let config = f.settings.cache.join("graphit-tsconfig.json");
        if provided {
            fs::write(&config, "EXACT CODE PROVIDED CONFIG").unwrap();
        }
        f.run().unwrap();
        if provided {
            assert_eq!(
                fs::read_to_string(&config).unwrap(),
                "EXACT CODE PROVIDED CONFIG"
            );
        } else {
            let config: Value = serde_json::from_slice(&fs::read(&config).unwrap()).unwrap();
            assert_eq!(config["compilerOptions"]["allowJs"], true);
            assert_eq!(config["exclude"].as_array().unwrap().len(), 4);
        }
        let calls = f.calls();
        assert_eq!(args(&calls[0]).last(), Some(&config.to_str().unwrap()));
    }
}

#[test]
fn python_native_toml_precedence_cache_and_input_failure_before_effects() {
    for (toml, version) in [
        (
            "[project]\nversion='2.3.4'\n[tool.poetry]\nversion='8.0.0'",
            "2.3.4",
        ),
        (
            "[project]\nversion=''\n[tool.poetry]\nversion='8.0.0'",
            "8.0.0",
        ),
        ("[project]\nname='fixture'", "0.0.0"),
    ] {
        let f = Fixture::new(Family::Python);
        f.input("pyproject.toml", toml);
        f.input("requirements.txt", "fixture");
        f.run().unwrap();
        let calls = f.calls();
        assert_eq!(calls[0]["tool"], "python3");
        assert_eq!(calls[1]["tool"], "pip");
        assert_eq!(
            args(calls.last().unwrap()).last().unwrap(),
            &format!("--project-version={version}")
        );
        f.run().unwrap();
        assert_eq!(
            f.calls()
                .iter()
                .filter(|call| call["tool"] == "python3")
                .count(),
            1
        );
    }
    for malformed in [
        "[project]\nversion=1",
        "[project",
        "[tool.poetry]\nversion=[]",
    ] {
        let f = Fixture::new(Family::Python);
        f.input("pyproject.toml", malformed);
        fs::write(&f.settings.output, "preserved").unwrap();
        assert_eq!(f.run(), Err(Error::Input));
        assert!(f.calls().is_empty());
        assert_eq!(fs::read(&f.settings.output).unwrap(), b"preserved");
    }
}

#[test]
fn clang_priority_filter_and_dotnet_sorted_restore_failure() {
    for (files, compdb) in [
        (
            vec![
                "compile_commands.json",
                "build/compile_commands.json",
                "CMakeLists.txt",
            ],
            "compile_commands.json",
        ),
        (
            vec!["build/compile_commands.json", "CMakeLists.txt"],
            "build/compile_commands.json",
        ),
        (vec!["CMakeLists.txt"], "cmake-build/compile_commands.json"),
    ] {
        let f = Fixture::new(Family::Clang);
        for file in files {
            f.input(file, "[]");
        }
        f.run().unwrap();
        let calls = f.calls();
        assert!(args(calls.last().unwrap())[0].ends_with(compdb));
    }
    let mut f = Fixture::new(Family::Clang);
    f.input("compile_commands.json", "[]");
    f.settings.host_root = Some("C:\\ação".into());
    f.run().unwrap();
    let calls = f.calls();
    assert_eq!(calls[0]["tool"], "graphit-filter-clang-compdb");
    assert_eq!(calls[0]["env"]["GRAPHIT_SCIP_HOST_ROOT"], "C:\\ação");
    assert_eq!(Fixture::new(Family::Clang).run(), Err(Error::Input));
    let f = Fixture::new(Family::Dotnet);
    for file in [
        "z.sln",
        "a.sln",
        "x.slnx",
        "b.csproj",
        "a.vbproj",
        ".hidden.sln",
    ] {
        f.input(file, "");
    }
    fs::create_dir(f.settings.workspace.join("directory.csproj")).unwrap();
    f.run().unwrap();
    let calls = f.calls();
    let restored: Vec<_> = calls
        .iter()
        .filter(|call| args(call).first() == Some(&"restore"))
        .map(|call| {
            Path::new(args(call)[1])
                .file_name()
                .unwrap()
                .to_str()
                .unwrap()
        })
        .collect();
    assert_eq!(
        restored,
        ["a.sln", "z.sln", "x.slnx", "b.csproj", "a.vbproj"]
    );
    let index = args(calls.last().unwrap());
    assert_eq!(index.len(), 9);
    assert!(index[4..].iter().all(|path| Path::new(path).is_absolute()));
    f.mode("dotnet", "exit=19");
    assert_eq!(f.run(), Err(Error::Child(19)));
    assert_eq!(f.calls().last().unwrap()["tool"], "dotnet");
    assert!(!f.settings.output.exists());
}

#[test]
fn dotnet_indexes_nested_projects_once_from_the_workspace_root() {
    let f = Fixture::new(Family::Dotnet);
    for file in [
        "tests/fixtures/vendor-dotnet/visualbasic/Corpus.vbproj",
        "tests/fixtures/vendor-dotnet/csharp/Corpus.csproj",
        "tests/fixtures/vendor-dotnet/visualbasic-constants/Corpus.vbproj",
        "tests/fixtures/vendor-dotnet/csharp-modern/Corpus.csproj",
    ] {
        f.input(file, "");
    }
    for excluded in [
        "bin/generated.csproj",
        "obj/generated.vbproj",
        "target/generated.csproj",
        "node_modules/generated.csproj",
        ".graphit/generated.csproj",
        "src/.hidden/generated.vbproj",
    ] {
        f.input(excluded, "");
    }
    f.run().unwrap();
    let calls = f.calls();
    assert_eq!(calls.len(), 5);
    let selected: Vec<_> = calls[..4].iter().map(|call| args(call)[1]).collect();
    let expected: Vec<_> = [
        "tests/fixtures/vendor-dotnet/csharp/Corpus.csproj",
        "tests/fixtures/vendor-dotnet/csharp-modern/Corpus.csproj",
        "tests/fixtures/vendor-dotnet/visualbasic/Corpus.vbproj",
        "tests/fixtures/vendor-dotnet/visualbasic-constants/Corpus.vbproj",
    ]
    .iter()
    .map(|path| f.settings.workspace.join(path))
    .collect();
    assert_eq!(
        selected,
        expected
            .iter()
            .map(|path| path.to_str().unwrap())
            .collect::<Vec<_>>()
    );
    assert_eq!(&args(calls.last().unwrap())[4..], selected);
    for call in calls {
        assert_eq!(call["cwd"], f.settings.workspace.to_str().unwrap());
        assert_eq!(
            call["env"]["DOTNET_CLI_HOME"],
            f.settings.cache.join("dotnet").to_str().unwrap()
        );
        assert_eq!(
            call["env"]["NUGET_PACKAGES"],
            f.settings.cache.join("nuget").to_str().unwrap()
        );
    }
}

#[test]
fn dotnet_uses_sdk_solution_membership_without_duplicate_project_inputs() {
    for extension in ["sln", "slnx"] {
        let f = Fixture::new(Family::Dotnet);
        f.input(&format!("solutions/Workspace.{extension}"), "SDK fixture");
        f.input("src/a ação/A.csproj", "");
        f.input("src/b/B.vbproj", "");
        f.input("standalone/C.csproj", "");
        f.mode(
            "dotnet",
            "projects=..\\src\\a ação\\A.csproj|../src/b/B.vbproj",
        );
        f.run().unwrap();
        let calls = f.calls();
        assert_eq!(calls.len(), 4);
        assert_eq!(args(&calls[0])[0], "sln");
        assert_eq!(args(&calls[0])[2], "list");
        let solution = f
            .settings
            .workspace
            .join(format!("solutions/Workspace.{extension}"));
        let standalone = f.settings.workspace.join("standalone/C.csproj");
        assert_eq!(
            args(&calls[1]),
            [
                "restore",
                solution.to_str().unwrap(),
                "/p:EnableWindowsTargeting=true"
            ]
        );
        assert_eq!(args(&calls[2])[1], standalone.to_str().unwrap());
        assert_eq!(
            &args(&calls[3])[4..],
            [solution.to_str().unwrap(), standalone.to_str().unwrap()]
        );
    }
}

#[test]
fn dotnet_rejects_empty_inputs_and_failed_or_oversized_solution_listing() {
    let f = Fixture::new(Family::Dotnet);
    fs::write(&f.settings.output, "preserved").unwrap();
    assert_eq!(f.run(), Err(Error::Input));
    assert!(f.calls().is_empty());
    assert_eq!(fs::read(&f.settings.output).unwrap(), b"preserved");
    f.input("nested/Workspace.sln", "");
    f.mode("dotnet", "exit=23");
    assert_eq!(f.run(), Err(Error::Child(23)));
    assert_eq!(f.calls().len(), 1);
    f.mode("dotnet", "huge=16385");
    assert_eq!(f.run(), Err(Error::Limit));
    assert_eq!(f.calls().len(), 2);
}

#[cfg(unix)]
#[test]
fn dotnet_discovery_does_not_follow_file_or_directory_links() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new(Family::Dotnet);
    let outside = f.root.join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("Outside.csproj"), "").unwrap();
    symlink(&outside, f.settings.workspace.join("linked")).unwrap();
    symlink(
        outside.join("Outside.csproj"),
        f.settings.workspace.join("link.csproj"),
    )
    .unwrap();
    symlink(&f.settings.workspace, f.settings.workspace.join("loop")).unwrap();
    f.input("nested/Actual.vbproj", "");
    f.run().unwrap();
    let calls = f.calls();
    assert_eq!(calls.len(), 2);
    assert_eq!(
        args(&calls[0])[1],
        f.settings
            .workspace
            .join("nested/Actual.vbproj")
            .to_str()
            .unwrap()
    );
}

#[test]
fn ruby_nil_metadata_sorbet_and_dart_cache_php_inputs() {
    for sorbet in [false, true] {
        let f = Fixture::new(Family::Ruby);
        f.input("a.gemspec", "");
        f.input("Gemfile", "");
        if sorbet {
            f.input("sorbet/config", "");
        }
        f.mode("ruby", "metadata=null");
        f.run().unwrap();
        let calls = f.calls();
        let last = args(calls.last().unwrap());
        assert!(last.contains(&"fixture ação \"name\"@0.0.0"));
        assert_eq!(last.last() == Some(&"."), !sorbet);
        let gemfile = fs::read_to_string(f.settings.cache.join("scip-ruby.Gemfile")).unwrap();
        assert!(gemfile.contains("ENV.fetch(\"GRAPHIT_SCIP_SOURCE_GEMFILE\")"));
        assert!(!gemfile.contains(f.root.to_str().unwrap()));
    }
    let f = Fixture::new(Family::Dart);
    f.run().unwrap();
    f.run().unwrap();
    assert_eq!(
        f.calls()
            .iter()
            .filter(|call| args(call).contains(&"activate"))
            .count(),
        1
    );
    let f = Fixture::new(Family::Php);
    assert_eq!(f.run(), Err(Error::Input));
    f.input("composer.json", "{}");
    assert_eq!(f.run(), Err(Error::Input));
    assert!(f.calls().is_empty());
}

#[test]
fn upstream_failure_missing_denied_empty_and_output_cleanup() {
    for (mode, error) in [
        ("exit=23", Error::Child(23)),
        ("output=missing", Error::Io),
        ("output=empty", Error::Io),
        ("output=malformed", Error::Input),
    ] {
        let f = Fixture::new(Family::Go);
        fs::write(&f.settings.output, "stale").unwrap();
        f.mode("scip-go", mode);
        assert_eq!(f.run(), Err(error));
    }
    let f = Fixture::new(Family::Go);
    fs::remove_file(f.root.join("bin/scip-go")).unwrap();
    assert_eq!(f.run(), Err(Error::MissingTool));
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let f = Fixture::new(Family::Go);
        fs::set_permissions(
            f.root.join("bin/scip-go"),
            fs::Permissions::from_mode(0o644),
        )
        .unwrap();
        assert_eq!(f.run(), Err(Error::DeniedTool));
    }
}

#[cfg(target_os = "linux")]
#[test]
fn branch_required_unicode_conversions_precede_every_output_or_child_effect() {
    use std::{ffi::OsString, os::unix::ffi::OsStringExt};
    for family in [Family::TypeScript, Family::Java] {
        let mut f = Fixture::new(family);
        let opaque = f.root.join(OsString::from_vec(vec![b'p', 0xff]));
        if family == Family::TypeScript {
            fs::rename(&f.settings.workspace, &opaque).unwrap();
            f.settings.workspace = opaque;
        } else {
            fs::rename(&f.settings.cache, &opaque).unwrap();
            f.settings.cache = opaque;
        }
        fs::write(&f.settings.output, b"previous-index").unwrap();
        assert_eq!(f.run(), Err(Error::Config));
        assert_eq!(fs::read(&f.settings.output).unwrap(), b"previous-index");
        assert!(f.calls().is_empty());
        // An opaque native path remains accepted when that family/branch uses argv.
        f.settings.family = Family::Go;
        f.run().unwrap();
        if family == Family::TypeScript {
            f.settings.family = Family::TypeScript;
            f.input("tsconfig.json", "{}");
            f.run().unwrap();
            fs::remove_file(f.settings.workspace.join("tsconfig.json")).unwrap();
            fs::write(f.settings.cache.join("graphit-tsconfig.json"), b"provided").unwrap();
            f.run().unwrap();
        }
    }
}

#[cfg(unix)]
#[test]
fn composed_python_path_and_join_errors_are_rejected_before_cache_output_or_children() {
    use dispatch::MAX_PATH_BYTES;
    let mut f = Fixture::new(Family::Python);
    let prefix = f
        .settings
        .cache
        .join("venv/bin")
        .as_os_str()
        .as_encoded_bytes()
        .len()
        + 1;
    f.settings.path = "a".repeat(MAX_PATH_BYTES + 1 - prefix).into();
    fs::write(&f.settings.output, b"previous-index").unwrap();
    assert_eq!(f.run(), Err(Error::Limit));
    assert_eq!(fs::read(&f.settings.output).unwrap(), b"previous-index");
    assert!(f.calls().is_empty());
    assert!(!f.settings.cache.join("venv").exists());
    let cache = f.root.join("cache:separator");
    fs::rename(&f.settings.cache, &cache).unwrap();
    f.settings.cache = cache;
    f.settings.path = f.root.join("bin").into_os_string();
    assert_eq!(f.run(), Err(Error::Config));
    assert_eq!(fs::read(&f.settings.output).unwrap(), b"previous-index");
    assert!(f.calls().is_empty());
    assert!(!f.settings.cache.join("venv").exists());
}

#[test]
fn cancellation_whole_deadline_and_concurrent_environment_isolation() {
    let f = Fixture::new(Family::Dart);
    // Each command fits the budget, but both together exceed it. Leave enough
    // launch margin for parallel tests while retaining the 3s cleanup bound.
    f.mode("dart", "sleep=800");
    let control = Control::with_timeout(Some(Duration::from_millis(1300))).unwrap();
    let start = Instant::now();
    assert_eq!(dispatch::run(&f.settings, &control), Err(Error::Deadline));
    assert!(start.elapsed() < Duration::from_secs(3));
    assert_eq!(f.calls().len(), 2);
    let f = Fixture::new(Family::Go);
    f.mode("scip-go", "sleep=10000");
    let control = Control::default();
    let cancelled = control.cancelled.clone();
    let cancel = thread::spawn(move || {
        thread::sleep(Duration::from_millis(60));
        cancelled.store(true, Ordering::Relaxed);
    });
    assert_eq!(dispatch::run(&f.settings, &control), Err(Error::Cancelled));
    cancel.join().unwrap();
    let first = Fixture::new(Family::Go);
    let second = Fixture::new(Family::Go);
    thread::scope(|scope| {
        let first = scope.spawn(|| first.run());
        let second = scope.spawn(|| second.run());
        first.join().unwrap().unwrap();
        second.join().unwrap().unwrap();
    });
    assert_ne!(
        first.calls()[0]["env"]["GOPATH"],
        second.calls()[0]["env"]["GOPATH"]
    );
}

#[cfg(unix)]
#[test]
fn leader_exit_with_descendant_pipe_and_metadata_overflow_are_bounded() {
    let f = Fixture::new(Family::Ruby);
    f.input("a.gemspec", "");
    let pid = f.root.join("descendant.pid");
    f.mode("ruby", &format!("descendant={}", pid.display()));
    let start = Instant::now();
    f.run().unwrap();
    assert!(start.elapsed() < Duration::from_secs(3));
    let pid: i32 = fs::read_to_string(pid).unwrap().parse().unwrap();
    // A terminated orphan can remain a zombie until its parent init reaps it.
    if let Ok(stat) = fs::read_to_string(format!("/proc/{pid}/stat")) {
        assert_eq!(
            stat.split(')').nth(1).unwrap().trim().chars().next(),
            Some('Z')
        );
    }
    let f = Fixture::new(Family::Ruby);
    f.input("a.gemspec", "");
    f.mode("ruby", "huge=16385");
    assert_eq!(f.run(), Err(Error::Limit));
    assert_eq!(f.calls().len(), 2);
}

#[test]
fn metadata_toml_discovery_caps_and_configuration_validation() {
    use dispatch::{MAX_DISCOVERY, MAX_METADATA_BYTES, MAX_TOML_BYTES};
    let mut f = Fixture::new(Family::Go);
    f.settings.project_name = "a".repeat(MAX_METADATA_BYTES);
    f.run().unwrap();
    f.settings.project_name.push('a');
    assert_eq!(f.run(), Err(Error::Limit));
    let f = Fixture::new(Family::Python);
    let path = f.settings.workspace.join("pyproject.toml");
    let file = fs::File::create(path).unwrap();
    file.set_len(MAX_TOML_BYTES as u64 + 1).unwrap();
    assert_eq!(f.run(), Err(Error::Limit));
    assert!(f.calls().is_empty());
    let f = Fixture::new(Family::Dotnet);
    for index in 0..MAX_DISCOVERY {
        f.input(&format!("{index}.csproj"), "");
    }
    f.mode("dotnet", "exit=31");
    assert_eq!(f.run(), Err(Error::Child(31)));
    assert_eq!(f.calls().len(), 1);
    f.input("one-more.csproj", "");
    assert_eq!(f.run(), Err(Error::Limit));
    assert_eq!(f.calls().len(), 1);
    assert_eq!(Family::parse("SECRET_UNKNOWN"), Err(Error::Config));
    assert_eq!(
        Control::with_timeout(Some(Duration::MAX)).err(),
        Some(Error::Config)
    );
    let output = Command::new(env!("CARGO_BIN_EXE_scip-run"))
        .arg("forbidden")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(2));
    assert!(!String::from_utf8_lossy(&output.stderr).contains("forbidden"));
    #[cfg(unix)]
    {
        use std::{ffi::OsString, os::unix::ffi::OsStringExt};
        let output = Command::new(env!("CARGO_BIN_EXE_scip-run"))
            .env("GRAPHIT_SCIP_FAMILY", "go")
            .env("GRAPHIT_SCIP_PROJECT_NAME", OsString::from_vec(vec![0xff]))
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(2));
        assert_eq!(output.stderr, b"invalid SCIP configuration\n");
    }
}

#[test]
fn captured_metadata_and_toml_exact_and_one_under_boundaries() {
    for limit in [16383, 16384] {
        let f = Fixture::new(Family::Ruby);
        f.input("a.gemspec", "");
        let mut metadata = serde_json::json!({"name":"", "version":"1"}).to_string();
        let overhead = metadata.len();
        metadata = serde_json::json!({"name":"a".repeat(limit-overhead),"version":"1"}).to_string();
        assert_eq!(metadata.len(), limit);
        f.mode("ruby", &format!("metadata={metadata}"));
        f.run().unwrap();
    }
    for size in [dispatch::MAX_TOML_BYTES - 1, dispatch::MAX_TOML_BYTES] {
        let f = Fixture::new(Family::Python);
        let mut content = String::from("#");
        content.extend(std::iter::repeat_n('a', size - 1));
        f.input("pyproject.toml", &content);
        f.run().unwrap();
    }
    let f = Fixture::new(Family::Python);
    f.input(
        "pyproject.toml",
        &format!(
            "[project]\nversion='{}'",
            "1".repeat(dispatch::MAX_METADATA_BYTES)
        ),
    );
    f.run().unwrap();
    f.input(
        "pyproject.toml",
        &format!(
            "[project]\nversion='{}'",
            "1".repeat(dispatch::MAX_METADATA_BYTES + 1)
        ),
    );
    assert_eq!(f.run(), Err(Error::Limit));
}

// Production images provide these Linux runtime paths; orchestration remains
// covered by the portable subprocess fixtures on Windows and macOS.
#[cfg(target_os = "linux")]
#[test]
fn real_ruby_gemspec_and_php_vendor_api_receive_literal_paths() {
    use std::os::unix::fs::symlink;
    let f = Fixture::new(Family::Ruby);
    fs::remove_file(f.root.join("bin/ruby")).unwrap();
    symlink("/usr/bin/ruby", f.root.join("bin/ruby")).unwrap();
    f.input(
        "a.gemspec",
        "Gem::Specification.new { |s| s.name='actual-ruby'; s.version='4.5.6' }",
    );
    f.input("Gemfile", "");
    f.run().unwrap();
    let calls = f.calls();
    assert!(args(calls.last().unwrap()).contains(&"actual-ruby@4.5.6"));
    f.input("Gemfile", "raise 'wrong literal source path' unless __dir__ == ENV.fetch('GRAPHIT_SCIP_EXPECTED_WORKSPACE')\n");
    assert!(
        Command::new("/usr/bin/ruby")
            .args([
                "-rbundler",
                "-e",
                "Bundler::Dsl.evaluate(ARGV.fetch(0), nil, {})"
            ])
            .arg(f.settings.cache.join("scip-ruby.Gemfile"))
            .env(
                "GRAPHIT_SCIP_SOURCE_GEMFILE",
                f.settings.workspace.join("Gemfile")
            )
            .env("GRAPHIT_SCIP_EXPECTED_WORKSPACE", &f.settings.workspace)
            .status()
            .unwrap()
            .success()
    );
    let f = Fixture::new(Family::Php);
    fs::remove_file(f.root.join("bin/php")).unwrap();
    symlink("/usr/bin/php", f.root.join("bin/php")).unwrap();
    f.input("composer.json", "{}");
    f.input("composer.lock", "{}");
    fs::write(&f.settings.php_autoload,r#"<?php namespace ScipPhp; class Indexer { public function __construct($root,$version,$options) { if ($root !== getcwd() || $version !== '0.0.1' || $options !== []) { exit(90); } } public function index() { return new class { public function serializeToString() { return "\x0a\x13\x1a\x11file:///workspace\x12\x0d\x0a\x09fixture.x\x30\x01"; } }; } }"#).unwrap();
    f.run().unwrap();
    let index =
        scip::types::Index::parse_from_bytes(&fs::read(&f.settings.output).unwrap()).unwrap();
    assert_eq!(index.documents[0].relative_path, "fixture.x");
}
