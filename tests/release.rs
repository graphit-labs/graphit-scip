use graphit_scip_tools::release::{self, Error, MAX_OUTPUT_BYTES};
use serde_json::Value;
use std::{
    fs,
    path::PathBuf,
    process::Command,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT: AtomicU64 = AtomicU64::new(0);
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        let root = std::env::temp_dir().join(format!(
            "graphit-release-test-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&root).unwrap();
        fs::create_dir(root.join("images")).unwrap();
        Self(root)
    }
    fn image(&self, name: &str) {
        let path = self.0.join("images").join(name);
        fs::create_dir(&path).unwrap();
        fs::write(path.join("Dockerfile"), b"FROM scratch\n").unwrap();
    }
    fn cli(&self, tag: &str, output: Option<&str>) -> std::process::Output {
        let mut command = Command::new(env!("CARGO_BIN_EXE_release-metadata"));
        command
            .current_dir(&self.0)
            .env("RELEASE_TAG", tag)
            .env_remove("GITHUB_OUTPUT");
        if let Some(output) = output {
            command.env("GITHUB_OUTPUT", self.0.join(output));
        }
        command.output().unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn stable_versions_platforms_and_sorted_discovery() {
    let f = Fixture::new();
    for name in ["ruby", "future-language", "clang", "go"] {
        f.image(name);
    }
    fs::create_dir(f.0.join("images/ignored")).unwrap();
    fs::create_dir(f.0.join("images/ignored/Dockerfile")).unwrap();
    fs::write(f.0.join("images/ordinary-file"), b"ignore").unwrap();
    for tag in [
        "0.0.0",
        "v0.2.1",
        "18446744073709551615.18446744073709551615.18446744073709551615",
    ] {
        let metadata = release::prepare(tag, &f.0.join("images")).unwrap();
        let json: Value =
            serde_json::from_slice(&metadata.json(MAX_OUTPUT_BYTES).unwrap()).unwrap();
        assert_eq!(json["patch"], tag.trim_start_matches('v'));
        let families: Vec<_> = json["matrix"]["include"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["family"].as_str().unwrap())
            .collect();
        assert_eq!(families, ["clang", "future-language", "go", "ruby"]);
        for row in json["matrix"]["include"].as_array().unwrap() {
            assert_eq!(
                row["platforms"],
                if matches!(row["family"].as_str().unwrap(), "clang" | "ruby") {
                    "linux/amd64"
                } else {
                    "linux/amd64,linux/arm64"
                }
            );
        }
        assert!(
            json["tags"]
                .as_str()
                .unwrap()
                .ends_with("type=raw,value=v1\ntype=raw,value=latest")
        );
    }
    for tag in [
        "",
        "v",
        "v01.2.3",
        "1.02.3",
        "1.2.03",
        "1.2",
        "1.2.3.4",
        "V1.2.3",
        "1.2.3-rc.1",
        "1.2.3+build",
        "18446744073709551616.0.0",
        "1.2.3\nSECRET=bad",
        " 1.2.3",
    ] {
        assert_eq!(
            release::prepare(tag, &f.0.join("images")).unwrap_err(),
            Error::Tag
        );
    }
    assert_eq!(
        release::prepare(&"a".repeat(65), &f.0.join("images")).unwrap_err(),
        Error::Tag
    );
    let boundary = format!("v{}.{}.{}", u64::MAX, u64::MAX, u64::MAX);
    assert_eq!(boundary.len(), 63);
    assert!(release::prepare(&boundary, &f.0.join("images")).is_ok());
    for extra in [" ", "  "] {
        assert_eq!(
            release::prepare(&(boundary.clone() + extra), &f.0.join("images")).unwrap_err(),
            Error::Tag
        );
    }
}

#[test]
fn admission_boundaries_and_render_before_output() {
    let f = Fixture::new();
    assert_eq!(
        release::prepare("1.2.3", &f.0.join("images")).unwrap_err(),
        Error::Empty
    );
    for i in 0..255 {
        f.image(&format!("family-{i:03}"));
    }
    assert!(release::prepare("1.2.3", &f.0.join("images")).is_ok());
    f.image(&"z".repeat(64));
    let metadata = release::prepare("1.2.3", &f.0.join("images")).unwrap();
    for github in [true, false] {
        let render = |limit| {
            if github {
                metadata.github_output(limit)
            } else {
                metadata.json(limit)
            }
        };
        let bytes = render(MAX_OUTPUT_BYTES).unwrap();
        assert_eq!(render(bytes.len()).unwrap(), bytes);
        assert_eq!(render(bytes.len() - 1), Err(Error::Limit));
        assert_eq!(render(0), Err(Error::Limit));
    }
    f.image("overflow");
    assert_eq!(
        release::prepare("1.2.3", &f.0.join("images")).unwrap_err(),
        Error::Limit
    );
    for name in [
        "a--b",
        "-bad",
        "bad-",
        "UpperCase",
        "bad name",
        &"a".repeat(65),
    ] {
        let f = Fixture::new();
        f.image(name);
        assert_eq!(
            release::prepare("1.2.3", &f.0.join("images")).unwrap_err(),
            Error::Family
        );
    }
}

#[test]
fn real_cli_outputs_are_consumable_and_validation_failures_leave_output_untouched() {
    let f = Fixture::new();
    f.image("clang");
    f.image("ruby");
    f.image("typescript");
    fs::write(f.0.join("github-output"), b"existing=unchanged\n").unwrap();
    for tag in [
        "1.2.3\nGRAPHIT_SCIP_RELEASE_TAGS\nSECRET=bad",
        "v01.2.3",
        &"x".repeat(65),
    ] {
        let output = f.cli(tag, Some("github-output"));
        assert_eq!(output.status.code(), Some(2));
        assert!(output.stdout.is_empty());
        assert!(!String::from_utf8_lossy(&output.stderr).contains("SECRET"));
        assert_eq!(
            fs::read(f.0.join("github-output")).unwrap(),
            b"existing=unchanged\n"
        );
    }
    let output = f.cli("v0.2.1", None);
    assert!(output.status.success());
    assert!(output.stderr.is_empty());
    let metadata: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(metadata["minor"], "0.2");
    assert_eq!(metadata["major"], "0");
    for tag in [None, Some("1.2.3")] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_release-metadata"));
        command
            .current_dir(&f.0)
            .env_remove("RELEASE_TAG")
            .env("GITHUB_OUTPUT", f.0.join("github-output"));
        if let Some(tag) = tag {
            command.env("RELEASE_TAG", tag).arg("unexpected");
        }
        let rejected = command.output().unwrap();
        assert_eq!(rejected.status.code(), Some(2));
        assert!(rejected.stdout.is_empty());
        assert_eq!(
            fs::read(f.0.join("github-output")).unwrap(),
            b"existing=unchanged\n"
        );
    }
    let output = f.cli("v0.2.1", Some("github-output"));
    assert!(output.status.success());
    let contents = fs::read_to_string(f.0.join("github-output")).unwrap();
    let lines: Vec<_> = contents.lines().collect();
    assert_eq!(lines[0], "existing=unchanged");
    let matrix: Value = serde_json::from_str(lines[1].strip_prefix("matrix=").unwrap()).unwrap();
    assert_eq!(matrix, metadata["matrix"]);
    assert_eq!(lines[2], "tags<<GRAPHIT_SCIP_RELEASE_TAGS");
    assert_eq!(lines[3..lines.len() - 1].join("\n"), metadata["tags"]);
    assert_eq!(lines.last().unwrap(), &"GRAPHIT_SCIP_RELEASE_TAGS");
    for family in ["clang", "ruby", "typescript"] {
        assert_eq!(
            fs::read(f.0.join("images").join(family).join("Dockerfile")).unwrap(),
            b"FROM scratch\n"
        );
    }
    let output = f.cli("1.2.3", Some("images/clang/Dockerfile"));
    assert_eq!(output.status.code(), Some(2));
    fs::hard_link(f.0.join("images/clang/Dockerfile"), f.0.join("alias")).unwrap();
    assert_eq!(f.cli("1.2.3", Some("alias")).status.code(), Some(2));
    assert_eq!(
        fs::read(f.0.join("images/clang/Dockerfile")).unwrap(),
        b"FROM scratch\n"
    );
    fs::remove_dir_all(f.0.join("images")).unwrap();
    assert_eq!(f.cli("1.2.3", Some("github-output")).status.code(), Some(2));
    assert_eq!(
        fs::read_to_string(f.0.join("github-output")).unwrap(),
        contents
    );
}

#[cfg(unix)]
#[test]
fn symlink_roots_families_dockerfiles_and_outputs_are_denied() {
    use std::os::unix::fs::symlink;
    for target in ["root", "family", "Dockerfile", "output"] {
        let f = Fixture::new();
        f.image("clang");
        fs::write(f.0.join("sentinel"), b"untouched").unwrap();
        match target {
            "root" => {
                fs::rename(f.0.join("images"), f.0.join("actual")).unwrap();
                symlink(f.0.join("actual"), f.0.join("images")).unwrap();
            }
            "family" => {
                fs::rename(f.0.join("images/clang"), f.0.join("actual")).unwrap();
                symlink(f.0.join("actual"), f.0.join("images/clang")).unwrap();
            }
            "Dockerfile" => {
                fs::remove_file(f.0.join("images/clang/Dockerfile")).unwrap();
                symlink(f.0.join("sentinel"), f.0.join("images/clang/Dockerfile")).unwrap();
            }
            _ => {
                symlink(f.0.join("sentinel"), f.0.join("github-output")).unwrap();
            }
        }
        assert_eq!(f.cli("1.2.3", Some("github-output")).status.code(), Some(2));
        assert_eq!(fs::read(f.0.join("sentinel")).unwrap(), b"untouched");
    }
}

#[test]
fn maintained_repository_matrix_and_workflow_consumers_match() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let metadata = release::prepare("v0.2.1", &root.join("images")).unwrap();
    let json: Value = serde_json::from_slice(&metadata.json(MAX_OUTPUT_BYTES).unwrap()).unwrap();
    let families: Vec<_> = json["matrix"]["include"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["family"].as_str().unwrap())
        .collect();
    assert_eq!(
        families,
        [
            "clang",
            "dart",
            "dotnet",
            "go",
            "java",
            "php",
            "python",
            "ruby",
            "rust",
            "typescript"
        ]
    );
    for family in &families {
        let dockerfile =
            fs::read_to_string(root.join("images").join(family).join("Dockerfile")).unwrap();
        assert!(dockerfile.contains("--bin scip-entrypoint"), "{family}");
        assert!(dockerfile.contains("COPY --from=graphit-tools /tools/target/release/scip-entrypoint /usr/local/bin/graphit-scip-index"), "{family}");
        assert!(!dockerfile.contains("scripts/entrypoint.sh"), "{family}");
    }
    let workflow = include_str!("../.github/workflows/release-images.yml");
    for consumer in [
        "uses: ./.github/workflows/rust.yml",
        "needs: [validation, version]",
        "cargo run --locked --release --bin release-metadata",
        "steps.release.outputs.matrix",
        "steps.release.outputs.tags",
        "fromJSON(needs.version.outputs.matrix)",
        "needs.version.outputs.tags",
        "matrix.platforms",
        "steps.meta.outputs.labels",
        "steps.meta.outputs.tags",
        "docker/login-action@v3",
        "docker/setup-qemu-action@v4",
        "docker/setup-buildx-action@v3",
        "docker/build-push-action@v6",
        "push: true",
    ] {
        assert!(workflow.contains(consumer), "{consumer}");
    }
    for obsolete in [
        "python3",
        "BASH_REMATCH",
        "case \"$FAMILY\"",
        "steps.platforms",
    ] {
        assert!(!workflow.contains(obsolete));
    }
}
