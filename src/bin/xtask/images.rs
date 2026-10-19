use serde_json::Value;
use std::{
    fs,
    path::Path,
    process::{Command, Stdio},
};

const FAMILIES: &[&str] = &[
    "go",
    "typescript",
    "python",
    "java",
    "dotnet",
    "rust",
    "ruby",
    "dart",
    "php",
];

fn checked(command: &mut Command) -> Result<(), String> {
    let program = command.get_program().to_string_lossy().into_owned();
    let status = command.status().map_err(|e| format!("{program}: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program}: {status}"))
    }
}

struct Fixture<'a> {
    image: &'a str,
    family: &'a str,
    source: &'a Path,
    cache: &'a Path,
    output: &'a Path,
}
impl Fixture<'_> {
    fn command(&self, prepare: bool) -> Result<Command, String> {
        let (uid, gid) = super::fixture_identity(self.source)?;
        let mut command = Command::new("docker");
        command.args(["run", "--rm", "--platform", "linux/amd64"]);
        for (path, target) in [
            (self.source, if prepare { "/workspace" } else { "/source" }),
            (self.cache, "/cache"),
            (self.output, "/output"),
        ] {
            let readonly = if path == self.source && !prepare {
                ",readonly"
            } else {
                ""
            };
            command.arg("--mount").arg(format!(
                "type=bind,src={},dst={target}{readonly}",
                path.display()
            ));
        }
        if prepare {
            command.args([
                "--user",
                &format!("{uid}:{gid}"),
                "--workdir",
                "/workspace",
                "--entrypoint",
                "/bin/sh",
            ]);
        } else {
            command.args([
                "--cap-add",
                "SYS_ADMIN",
                "--security-opt",
                "apparmor=unconfined",
                "--tmpfs",
                "/overlay:rw,nosuid,nodev,size=1g,mode=1777",
                "-e",
                "GRAPHIT_SCIP_OVERLAY=1",
                "-e",
                &format!("GRAPHIT_SCIP_UID={uid}"),
                "-e",
                &format!("GRAPHIT_SCIP_GID={gid}"),
                "-e",
                &format!("GRAPHIT_SCIP_HOST_ROOT={}", self.source.display()),
            ]);
        }
        command.args([
            "-e",
            "DOTNET_CLI_WORKLOAD_UPDATE_NOTIFY_DISABLE=true",
            "-e",
            "DOTNET_SKIP_FIRST_TIME_EXPERIENCE=1",
            "-e",
            "HOME=/cache/home",
            "-e",
            "NUGET_PACKAGES=/cache/nuget",
            "-e",
            "DOTNET_CLI_HOME=/cache/dotnet",
            "-e",
            "PUB_CACHE=/cache/pub",
            "-e",
            "COMPOSER_HOME=/cache/composer",
            "-e",
            "COMPOSER_CACHE_DIR=/cache/composer-cache",
        ]);
        Ok(command)
    }
    fn prepare(&self) -> Result<(), String> {
        let script = match self.family {
            "dotnet" => "timeout 120s dotnet restore -p:NuGetAudit=false",
            "dart" => "dart pub get --offline",
            "php" => "composer update --no-interaction --prefer-dist",
            _ => return Ok(()),
        };
        checked(self.command(true)?.args([self.image, "-c", script]))
    }
    fn invoke(&self, selected: &str, timeout: &str, expected: i32) -> Result<(), String> {
        let status = self
            .command(false)?
            .args([
                "-e",
                &format!("GRAPHIT_SCIP_FAMILY={selected}"),
                "-e",
                &format!("GRAPHIT_SCIP_TIMEOUT_MS={timeout}"),
                self.image,
            ])
            .status()
            .map_err(|e| e.to_string())?;
        if status.code() == Some(expected) {
            Ok(())
        } else {
            Err(format!(
                "{} expected exit {expected}, got {status}",
                self.family
            ))
        }
    }
}

fn index_check(index: &Path, expected: &str) -> Result<(), String> {
    if fs::metadata(index).map_err(|e| e.to_string())?.len() == 0 {
        return Err("empty SCIP index".into());
    }
    let decoded = index.with_extension("json");
    checked(
        Command::new("scip")
            .args(["print", "--json"])
            .arg(index)
            .stdout(Stdio::from(
                fs::File::create(&decoded).map_err(|e| e.to_string())?,
            )),
    )?;
    let data: Value = serde_json::from_slice(&fs::read(decoded).map_err(|e| e.to_string())?)
        .map_err(|e| e.to_string())?;
    let doc = data["documents"]
        .as_array()
        .and_then(|docs| {
            docs.iter()
                .find(|doc| doc["relative_path"] == expected || doc["relativePath"] == expected)
        })
        .ok_or("expected source document absent")?;
    if !doc["occurrences"].as_array().is_some_and(|occurrences| {
        occurrences.iter().any(|occurrence| {
            occurrence["symbol"].as_str().is_some_and(|s| !s.is_empty())
                && nonempty_range(occurrence)
        })
    }) {
        return Err("SCIP symbol/range occurrence absent".into());
    }
    checked(Command::new("scip").arg("lint").arg(index))
}

fn smoke(image: &str, family: &str, corpus: Option<(&str, &str)>) -> Result<(), String> {
    let scratch = super::Scratch::new()?;
    let source = scratch.0.join("source");
    let cache = scratch.0.join("cache");
    let output = scratch.0.join("output");
    for path in [
        &source,
        &cache.join("home"),
        &cache.join("composer"),
        &output,
    ] {
        fs::create_dir_all(path).map_err(|e| e.to_string())?;
    }
    let expected = if let Some((directory, expected)) = corpus {
        super::copy_tree(
            &Path::new("tests/fixtures/vendor-dotnet").join(directory),
            &source,
        )?;
        expected
    } else {
        fixture(&source, family)?
    };
    let fixture = Fixture {
        image,
        family,
        source: &source,
        cache: &cache,
        output: &output,
    };
    fixture.prepare()?;
    // Original fixture sources remain untouched; generated restore inputs are disposable.
    let original = fs::read(source.join(expected)).map_err(|e| e.to_string())?;
    let index = output.join("index.scip");
    fixture.invoke(family, "240000", 0)?;
    index_check(&index, expected)?;
    fs::write(cache.join("reuse-marker"), "retained").map_err(|e| e.to_string())?;
    fixture.invoke(family, "240000", 0)?;
    index_check(&index, expected)?;
    if fs::read(cache.join("reuse-marker")).map_err(|e| e.to_string())? != b"retained"
        || fs::read(source.join(expected)).map_err(|e| e.to_string())? != original
    {
        return Err("source/cache contract violated".into());
    }
    // Configuration/deadline behavior is shared across the .NET corpus.
    if corpus.is_none_or(|(name, _)| name == "csharp") {
        let accepted = fs::read(&index).map_err(|e| e.to_string())?;
        fixture.invoke("invalid", "240000", 2)?;
        if fs::read(&index).map_err(|e| e.to_string())? != accepted {
            return Err("configuration failure changed accepted output".into());
        }
        fixture.invoke(family, "1", 124)?;
        if index.exists() {
            return Err("deadline left stale index".into());
        }
    }
    println!(
        "{family} {}: cold/warm SCIP, cache reuse, source preservation and error exits passed",
        corpus.map_or("smoke", |(name, _)| name)
    );
    Ok(())
}

fn dotnet_failure(image: &str) -> Result<(), String> {
    let scratch = super::Scratch::new()?;
    let source = scratch.0.join("source");
    let cache = scratch.0.join("cache");
    let output = scratch.0.join("output");
    for path in [&source, &cache.join("home"), &output] {
        fs::create_dir_all(path).map_err(|e| e.to_string())?;
    }
    write(
        &source,
        "Invalid.csproj",
        "<Project Sdk=\"Microsoft.NET.Sdk\"><InvalidXml>",
    )?;
    let fixture = Fixture {
        image,
        family: "dotnet",
        source: &source,
        cache: &cache,
        output: &output,
    };
    let status = fixture
        .command(false)?
        .args([
            "-e",
            "GRAPHIT_SCIP_FAMILY=dotnet",
            "-e",
            "GRAPHIT_SCIP_TIMEOUT_MS=30000",
            image,
        ])
        .status()
        .map_err(|e| e.to_string())?;
    if status.success() || output.join("index.scip").exists() {
        return Err("invalid .NET project published a successful index".into());
    }
    println!("dotnet: invalid project rejected without an index");
    Ok(())
}

pub fn run(family: Option<&str>, existing_image: Option<&str>) -> Result<(), String> {
    if family.is_some_and(|f| !FAMILIES.contains(&f))
        || (existing_image.is_some() && family.is_none())
    {
        return Err("select a known image family before supplying an image ID".into());
    }
    if let Some(image) = existing_image {
        let valid = image.strip_prefix("sha256:").is_some_and(|digest| {
            digest.len() == 64
                && digest
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        });
        if !valid {
            return Err("expected sha256:imageID".into());
        }
    }
    for family in FAMILIES
        .iter()
        .copied()
        .filter(|f| family.is_none_or(|selected| selected == *f))
    {
        let image = existing_image.map_or_else(
            || format!("graphit-scip-{family}:rust-check"),
            str::to_owned,
        );
        if existing_image.is_none() {
            checked(Command::new("docker").args([
                "build",
                "--platform",
                "linux/amd64",
                "-t",
                &image,
                "-f",
                &format!("images/{family}/Dockerfile"),
                ".",
            ]))?;
        }
        if family == "dotnet" {
            for (directory, expected) in [
                ("csharp-modern", "Modern.cs"),
                ("visualbasic-constants", "Conditional.vb"),
                ("csharp", "Global.cs"),
                ("visualbasic", "Global.vb"),
            ] {
                smoke(&image, family, Some((directory, expected)))?;
            }
            dotnet_failure(&image)?;
        } else {
            smoke(&image, family, None)?;
        }
    }
    Ok(())
}

fn write(root: &Path, name: &str, contents: &str) -> Result<(), String> {
    let path = root.join(name);
    fs::create_dir_all(path.parent().ok_or("fixture parent")?).map_err(|e| e.to_string())?;
    fs::write(path, contents).map_err(|e| e.to_string())
}
fn fixture(root: &Path, family: &str) -> Result<&'static str, String> {
    let (files, expected): (Vec<(&str, &str)>, &str) = match family {
        "go" => (
            vec![
                ("go.mod", "module example.com/graphitfixture\n\ngo 1.26\n"),
                (
                    "keep.go",
                    "package fixture\nfunc Keep() int { return 42 }\n",
                ),
            ],
            "keep.go",
        ),
        "typescript" => (
            vec![
                (
                    "package.json",
                    r#"{"name":"graphit-fixture","version":"0.1.0"}"#,
                ),
                (
                    "tsconfig.json",
                    r#"{"compilerOptions":{"target":"ES2022","module":"commonjs"},"include":["keep.ts"]}"#,
                ),
                ("keep.ts", "export function keep(): number { return 42; }\n"),
            ],
            "keep.ts",
        ),
        "python" => (
            vec![
                (
                    "pyproject.toml",
                    "[project]\nname = \"graphit-fixture\"\nversion = \"0.1.0\"\n",
                ),
                ("keep.py", "def keep() -> int:\n    return 42\n"),
            ],
            "keep.py",
        ),
        "java" => (
            vec![
                (
                    "pom.xml",
                    r#"<project xmlns="http://maven.apache.org/POM/4.0.0"><modelVersion>4.0.0</modelVersion><groupId>example</groupId><artifactId>graphit-fixture</artifactId><version>0.1.0</version><properties><maven.compiler.release>17</maven.compiler.release></properties><build><plugins><plugin><groupId>org.apache.maven.plugins</groupId><artifactId>maven-compiler-plugin</artifactId><version>3.14.1</version></plugin></plugins></build></project>"#,
                ),
                (
                    "src/main/java/Keep.java",
                    "public class Keep { public static int keep() { return 42; } }\n",
                ),
            ],
            "src/main/java/Keep.java",
        ),
        "dotnet" => (
            vec![
                (
                    "Fixture.csproj",
                    "<Project Sdk=\"Microsoft.NET.Sdk\"><PropertyGroup><TargetFramework>net10.0</TargetFramework><NuGetAudit>false</NuGetAudit></PropertyGroup></Project>",
                ),
                (
                    "Keep.cs",
                    "public class Keep { public int Value => new Drop().Value; }\n",
                ),
                ("Drop.cs", "public class Drop { public int Value => 42; }\n"),
            ],
            "Keep.cs",
        ),
        "rust" => (
            vec![
                (
                    "Cargo.toml",
                    "[package]\nname = \"graphit-fixture\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
                ),
                ("src/lib.rs", "pub fn keep() -> i32 { 42 }\n"),
            ],
            "src/lib.rs",
        ),
        "ruby" => (
            vec![
                ("Gemfile", "source 'https://rubygems.org'\ngemspec\n"),
                (
                    "graphit_fixture.gemspec",
                    "Gem::Specification.new do |s|\n s.name = 'graphit_fixture'\n s.version = '0.1.0'\n s.summary = 'SCIP fixture'\n s.authors = ['Fixture']\n s.files = ['lib/keep.rb']\nend\n",
                ),
                (
                    "lib/keep.rb",
                    "module Fixture\n def self.keep\n  42\n end\nend\n",
                ),
            ],
            "lib/keep.rb",
        ),
        "dart" => (
            vec![
                (
                    "pubspec.yaml",
                    "name: graphit_fixture\nversion: 0.1.0\nenvironment:\n  sdk: '>=3.6.0 <4.0.0'\n",
                ),
                ("lib/keep.dart", "int keep() => 42;\n"),
            ],
            "lib/keep.dart",
        ),
        "php" => (
            vec![
                (
                    "composer.json",
                    r#"{"name":"example/graphit-fixture","version":"0.1.0","autoload":{"psr-4":{"Fixture\\":"src/"}}}"#,
                ),
                (
                    "composer.lock",
                    r#"{"_readme":["Qualification fixture with no dependencies"],"content-hash":"","packages":[],"packages-dev":[],"aliases":[],"minimum-stability":"stable","stability-flags":{},"prefer-stable":false,"prefer-lowest":false,"platform":{},"platform-dev":{},"plugin-api-version":"2.6.0"}"#,
                ),
                (
                    "src/Keep.php",
                    "<?php\nnamespace Fixture;\nclass Keep { public static function value(): int { return 42; } }\n",
                ),
            ],
            "src/Keep.php",
        ),
        _ => return Err("unknown image family".into()),
    };
    for (name, contents) in files {
        write(root, name, contents)?;
    }
    Ok(expected)
}
fn nonempty_range(occurrence: &Value) -> bool {
    fn number(value: &Value, key: &str) -> Option<u64> {
        value.get(key).map_or(Some(0), Value::as_u64)
    }
    let typed = &occurrence["TypedRange"];
    let bounds = if let Some(range) = typed.get("SingleLineRange") {
        number(range, "line")
            .zip(number(range, "start_character"))
            .zip(number(range, "end_character"))
            .map(|((line, start), end)| (line, start, line, end))
    } else if let Some(range) = typed.get("MultiLineRange") {
        number(range, "start_line")
            .zip(number(range, "start_character"))
            .zip(number(range, "end_line"))
            .zip(number(range, "end_character"))
            .map(|(((sl, sc), el), ec)| (sl, sc, el, ec))
    } else if typed.is_null() {
        occurrence["range"]
            .as_array()
            .and_then(|r| match r.as_slice() {
                [sl, sc, ec] => sl
                    .as_u64()
                    .zip(sc.as_u64())
                    .zip(ec.as_u64())
                    .map(|((sl, sc), ec)| (sl, sc, sl, ec)),
                [sl, sc, el, ec] => sl
                    .as_u64()
                    .zip(sc.as_u64())
                    .zip(el.as_u64())
                    .zip(ec.as_u64())
                    .map(|(((sl, sc), el), ec)| (sl, sc, el, ec)),
                _ => None,
            })
    } else {
        None
    };
    bounds.is_some_and(|(sl, sc, el, ec)| (sl, sc) < (el, ec))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    #[test]
    fn fixture_command_keeps_source_isolated_and_paths_literal() {
        let scratch = super::super::Scratch::new().unwrap();
        let source = scratch.0.join("source with spaces β");
        let cache = scratch.0.join("cache");
        let output = scratch.0.join("output");
        for directory in [&source, &cache, &output] {
            fs::create_dir(directory).unwrap();
        }
        let fixture = Fixture {
            image: "unused",
            family: "go",
            source: &source,
            cache: &cache,
            output: &output,
        };
        for prepare in [false, true] {
            let command = fixture.command(prepare).unwrap();
            assert_eq!(command.get_program(), "docker");
            let args: Vec<_> = command
                .get_args()
                .map(|arg| arg.to_str().unwrap())
                .collect();
            let mounts: Vec<_> = args
                .windows(2)
                .filter(|pair| pair[0] == "--mount")
                .map(|pair| pair[1])
                .collect();
            let expected = [
                format!(
                    "type=bind,src={},dst={}{}",
                    source.display(),
                    if prepare { "/workspace" } else { "/source" },
                    if prepare { "" } else { ",readonly" }
                ),
                format!("type=bind,src={},dst=/cache", cache.display()),
                format!("type=bind,src={},dst=/output", output.display()),
            ];
            assert_eq!(mounts, expected);
            assert!(
                args.windows(2)
                    .any(|pair| pair == ["-e", "HOME=/cache/home"])
            );
            if prepare {
                assert!(
                    args.windows(2)
                        .any(|pair| pair == ["--workdir", "/workspace"])
                );
                assert!(
                    args.windows(2)
                        .any(|pair| pair == ["--entrypoint", "/bin/sh"])
                );
                assert!(!args.contains(&"GRAPHIT_SCIP_OVERLAY=1"));
            } else {
                assert!(
                    args.windows(2)
                        .any(|pair| pair
                            == ["--tmpfs", "/overlay:rw,nosuid,nodev,size=1g,mode=1777"])
                );
                assert!(
                    args.windows(2)
                        .any(|pair| pair == ["-e", "GRAPHIT_SCIP_OVERLAY=1"])
                );
            }
        }
    }
    #[test]
    fn upstream_typed_ranges_take_precedence_and_reject_invalid_positions() {
        let java: Value = serde_json::from_str(include_str!(
            "../../../tests/fixtures/dispatch/java-decoded.json"
        ))
        .unwrap();
        assert!(
            java["documents"][0]["occurrences"]
                .as_array()
                .unwrap()
                .iter()
                .all(nonempty_range)
        );
        let bad = json!({"range":[0,1,8],"TypedRange":{"SingleLineRange":{"start_character":8,"end_character":1}}});
        assert!(!nonempty_range(&bad));
        assert!(!nonempty_range(
            &json!({"TypedRange":{"SingleLineRange":{"start_character":-1,"end_character":2}}})
        ));
        assert!(nonempty_range(
            &json!({"TypedRange":{"MultiLineRange":{"start_line":0,"end_line":2,"end_character":3}}})
        ));
        assert!(!nonempty_range(
            &json!({"TypedRange":{"MultiLineRange":{"start_line":2,"end_line":0,"end_character":3}}})
        ));
    }
}
