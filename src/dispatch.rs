//! Bounded orchestration of the maintained upstream SCIP indexers.
mod encoding;
mod process;

use serde::Deserialize;
use std::{
    ffi::{OsStr, OsString},
    fmt, fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::Command,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};

pub const MAX_METADATA_BYTES: usize = 32 * 1024;
pub const MAX_PATH_BYTES: usize = 128 * 1024;
pub const MAX_TOML_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_CONFIG_BYTES: usize = 64 * 1024;
pub const MAX_DISCOVERY: usize = 4096;
pub(super) const MAX_CAPTURE_BYTES: usize = 16 * 1024;

const RUBY_METADATA: &str = "require 'json'; s=Gem::Specification.load(ARGV.fetch(0)); print JSON.generate(s ? {name:s.name,version:s.version.to_s} : nil)";
const PHP_INDEX: &str = "require $argv[1]; $indexer = new \\ScipPhp\\Indexer($argv[2], \"0.0.1\", []); if (file_put_contents($argv[3], $indexer->index()->serializeToString()) === false) { exit(1); }";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Family {
    Go,
    TypeScript,
    Python,
    Java,
    Clang,
    Dotnet,
    Rust,
    Ruby,
    Dart,
    Php,
}
impl Family {
    pub fn parse(value: &str) -> Result<Self, Error> {
        match value {
            "go" => Ok(Self::Go),
            "typescript" => Ok(Self::TypeScript),
            "python" => Ok(Self::Python),
            "java" => Ok(Self::Java),
            "clang" => Ok(Self::Clang),
            "dotnet" => Ok(Self::Dotnet),
            "rust" => Ok(Self::Rust),
            "ruby" => Ok(Self::Ruby),
            "dart" => Ok(Self::Dart),
            "php" => Ok(Self::Php),
            _ => Err(Error::Config),
        }
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub enum Error {
    Config,
    Input,
    Limit,
    Io,
    MissingTool,
    DeniedTool,
    Child(i32),
    Cancelled,
    Deadline,
}
impl Error {
    pub fn exit_code(self) -> i32 {
        match self {
            Self::Config | Self::Input | Self::Limit => 2,
            Self::Io => 1,
            Self::MissingTool => 127,
            Self::DeniedTool => 126,
            Self::Child(code) => code,
            Self::Cancelled => 130,
            Self::Deadline => 124,
        }
    }
}
impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Config => "invalid SCIP configuration",
            Self::Input => "required SCIP input is missing or invalid",
            Self::Limit => "SCIP admission limit exceeded",
            Self::Io => "SCIP execution or output failed",
            Self::MissingTool => "required SCIP executable is missing",
            Self::DeniedTool => "SCIP executable permission denied",
            Self::Child(_) => "upstream SCIP command failed",
            Self::Cancelled => "SCIP execution cancelled",
            Self::Deadline => "SCIP execution deadline exceeded",
        })
    }
}
impl std::error::Error for Error {}

#[derive(Clone, Default)]
pub struct Control {
    pub cancelled: Arc<AtomicBool>,
    pub deadline: Option<Instant>,
}
impl Control {
    pub fn with_timeout(timeout: Option<Duration>) -> Result<Self, Error> {
        let deadline = timeout
            .map(|duration| Instant::now().checked_add(duration).ok_or(Error::Config))
            .transpose()?;
        Ok(Self {
            deadline,
            ..Self::default()
        })
    }
    pub fn check(&self) -> Result<(), Error> {
        if self.cancelled.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        if self
            .deadline
            .is_some_and(|deadline| Instant::now() >= deadline)
        {
            return Err(Error::Deadline);
        }
        Ok(())
    }
}

/// Every invocation owns its PATH and child environment; no process-global mutation.
pub struct Settings {
    pub family: Family,
    pub workspace: PathBuf,
    pub cache: PathBuf,
    pub output: PathBuf,
    pub path: OsString,
    pub host_root: Option<OsString>,
    pub project_name: String,
    pub php_autoload: PathBuf,
}
impl Settings {
    pub fn container() -> Result<Self, Error> {
        let family = std::env::var("GRAPHIT_SCIP_FAMILY").map_err(|_| Error::Config)?;
        let project_name = match std::env::var("GRAPHIT_SCIP_PROJECT_NAME") {
            Ok(name) if !name.is_empty() => name,
            Ok(_) | Err(std::env::VarError::NotPresent) => "workspace".into(),
            Err(std::env::VarError::NotUnicode(_)) => return Err(Error::Config),
        };
        let settings = Self {
            family: Family::parse(&family)?,
            workspace: "/workspace".into(),
            cache: "/cache".into(),
            output: "/output/index.scip".into(),
            path: std::env::var_os("PATH").unwrap_or_default(),
            host_root: std::env::var_os("GRAPHIT_SCIP_HOST_ROOT").filter(|value| !value.is_empty()),
            project_name,
            php_autoload: "/opt/scip-php/vendor/autoload.php".into(),
        };
        settings.validate()?;
        Ok(settings)
    }
    fn validate(&self) -> Result<(), Error> {
        for path in [
            &self.workspace,
            &self.cache,
            &self.output,
            &self.php_autoload,
        ] {
            bounded_os(path.as_os_str(), MAX_PATH_BYTES)?;
            if !path.is_absolute() {
                return Err(Error::Config);
            }
        }
        bounded_os(&self.path, MAX_PATH_BYTES)?;
        if let Some(root) = &self.host_root {
            bounded_os(root, MAX_METADATA_BYTES)?;
        }
        bounded_text(&self.project_name)?;
        if !self.workspace.is_dir()
            || !self.cache.is_dir()
            || !self.output.parent().is_some_and(Path::is_dir)
        {
            return Err(Error::Input);
        }
        // OUTPUT cannot remove a source root or a cache directory.
        if self.output == self.workspace || self.output == self.cache {
            return Err(Error::Config);
        }
        Ok(())
    }
}

fn bounded_os(value: &OsStr, limit: usize) -> Result<(), Error> {
    if value.as_encoded_bytes().len() > limit {
        return Err(Error::Limit);
    }
    if value.as_encoded_bytes().contains(&0) {
        return Err(Error::Config);
    }
    Ok(())
}
fn bounded_text(value: &str) -> Result<(), Error> {
    bounded_os(OsStr::new(value), MAX_METADATA_BYTES)
}
fn regular(path: &Path) -> Result<bool, Error> {
    match fs::metadata(path) {
        Ok(metadata) => Ok(metadata.is_file()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(_) => Err(Error::Io),
    }
}
fn executable(path: &Path) -> Result<bool, Error> {
    if !regular(path)? {
        return Ok(false);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        Ok(fs::metadata(path)
            .map_err(|_| Error::Io)?
            .permissions()
            .mode()
            & 0o111
            != 0)
    }
    #[cfg(not(unix))]
    {
        Ok(true)
    }
}
fn candidates(
    workspace: &Path,
    extensions: &[&str],
    recursive: bool,
    control: &Control,
) -> Result<Vec<PathBuf>, Error> {
    let mut candidates = Vec::new();
    let mut directories = vec![workspace.to_owned()];
    while let Some(directory) = directories.pop() {
        control.check()?;
        for entry in fs::read_dir(directory).map_err(|_| Error::Io)? {
            control.check()?;
            let entry = entry.map_err(|_| Error::Io)?;
            let name = entry.file_name();
            if name.as_encoded_bytes().starts_with(b".") {
                continue;
            }
            let kind = entry.file_type().map_err(|_| Error::Io)?;
            let path = entry.path();
            bounded_os(path.as_os_str(), MAX_PATH_BYTES)?;
            // Never follow links out of the source tree, or rediscover generated projects.
            if recursive && kind.is_dir() {
                if !["bin", "obj", "node_modules", "target"].contains(&name.to_str().unwrap_or(""))
                {
                    directories.push(path);
                }
            } else if path
                .extension()
                .is_some_and(|extension| extensions.iter().any(|wanted| extension == *wanted))
                && (kind.is_file() || (!recursive && kind.is_symlink() && regular(&path)?))
            {
                if candidates.len() == MAX_DISCOVERY {
                    return Err(Error::Limit);
                }
                candidates.try_reserve(1).map_err(|_| Error::Limit)?;
                candidates.push(path);
            }
        }
    }
    candidates.sort_by(|left, right| {
        let group = |path: &Path| {
            extensions
                .iter()
                .position(|extension| path.extension() == Some(OsStr::new(extension)))
                .unwrap_or(extensions.len())
        };
        group(left).cmp(&group(right)).then_with(|| left.cmp(right))
    });
    Ok(candidates)
}

fn dotnet_inputs(runner: &Runner<'_>, discovered: Vec<PathBuf>) -> Result<Vec<PathBuf>, Error> {
    let mut covered = std::collections::HashSet::new();
    for solution in discovered.iter().filter(|path| {
        path.extension()
            .is_some_and(|extension| extension == "sln" || extension == "slnx")
    }) {
        // Let the SDK read both solution formats; do not maintain a second SLN parser.
        let mut command = runner.command("dotnet");
        command.arg("sln").arg(solution).arg("list");
        let bytes = process::execute(command, runner.control, true)?;
        let listing = std::str::from_utf8(&bytes).map_err(|_| Error::Input)?;
        for line in listing.lines() {
            runner.control.check()?;
            let path = PathBuf::from(line.replace('\\', "/"));
            if !path
                .extension()
                .is_some_and(|extension| extension == "csproj" || extension == "vbproj")
            {
                continue;
            }
            let path = solution.parent().ok_or(Error::Input)?.join(path);
            // Normalize SDK-relative '..' without following links during discovery.
            let mut normalized = PathBuf::new();
            for component in path.components() {
                match component {
                    std::path::Component::CurDir => (),
                    std::path::Component::ParentDir => {
                        normalized.pop();
                    }
                    other => normalized.push(other),
                }
            }
            covered.insert(normalized);
        }
    }
    Ok(discovered
        .into_iter()
        .filter(|path| !covered.contains(path))
        .collect())
}
fn bounded_read(path: &Path, limit: usize, control: &Control) -> Result<Vec<u8>, Error> {
    control.check()?;
    let mut file = fs::File::open(path).map_err(|_| Error::Io)?;
    if file.metadata().map_err(|_| Error::Io)?.len() > limit as u64 {
        return Err(Error::Limit);
    }
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        control.check()?;
        let count = file.read(&mut chunk).map_err(|_| Error::Io)?;
        if count == 0 {
            return Ok(bytes);
        }
        if bytes
            .len()
            .checked_add(count)
            .is_none_or(|size| size > limit)
        {
            return Err(Error::Limit);
        }
        bytes.try_reserve(count).map_err(|_| Error::Limit)?;
        bytes.extend_from_slice(&chunk[..count]);
    }
}
#[derive(Default, Deserialize)]
struct PythonProject {
    project: Option<Version>,
    tool: Option<PythonTool>,
}
#[derive(Default, Deserialize)]
struct Version {
    version: Option<VersionString>,
}
struct VersionString(Option<String>);
impl<'de> Deserialize<'de> for VersionString {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct Bounded;
        impl serde::de::Visitor<'_> for Bounded {
            type Value = VersionString;
            fn expecting(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
                formatter.write_str("a project version string")
            }
            fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
                Ok(VersionString(
                    (value.len() <= MAX_METADATA_BYTES).then(|| value.to_owned()),
                ))
            }
            fn visit_string<E: serde::de::Error>(self, value: String) -> Result<Self::Value, E> {
                Ok(VersionString(
                    (value.len() <= MAX_METADATA_BYTES).then_some(value),
                ))
            }
        }
        deserializer.deserialize_string(Bounded)
    }
}
#[derive(Default, Deserialize)]
struct PythonTool {
    poetry: Option<Version>,
}
fn python_version(settings: &Settings, control: &Control) -> Result<String, Error> {
    let path = settings.workspace.join("pyproject.toml");
    if !regular(&path)? {
        return Ok("0.0.0".into());
    }
    let bytes = bounded_read(&path, MAX_TOML_BYTES, control)?;
    let text = std::str::from_utf8(&bytes).map_err(|_| Error::Input)?;
    let project: PythonProject = toml::from_str(text).map_err(|_| Error::Input)?;
    control.check()?;
    let project_version = project.project.and_then(|project| project.version);
    let poetry_version = project
        .tool
        .and_then(|tool| tool.poetry)
        .and_then(|poetry| poetry.version);
    if project_version
        .as_ref()
        .is_some_and(|version| version.0.is_none())
        || poetry_version
            .as_ref()
            .is_some_and(|version| version.0.is_none())
    {
        return Err(Error::Limit);
    }
    let version = project_version
        .and_then(|version| version.0)
        .filter(|version| !version.is_empty())
        .or_else(|| {
            poetry_version
                .and_then(|version| version.0)
                .filter(|version| !version.is_empty())
        })
        .unwrap_or_else(|| "0.0.0".into());
    bounded_text(&version)?;
    Ok(version)
}

struct ConfigWriter(Vec<u8>);
impl Write for ConfigWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        if self
            .0
            .len()
            .checked_add(bytes.len())
            .is_none_or(|size| size > MAX_CONFIG_BYTES)
        {
            return Err(std::io::Error::other("configuration limit"));
        }
        self.0
            .try_reserve(bytes.len())
            .map_err(std::io::Error::other)?;
        self.0.extend_from_slice(bytes);
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

fn typescript_config(settings: &Settings) -> Result<Option<Vec<u8>>, Error> {
    if regular(&settings.workspace.join("tsconfig.json"))?
        || regular(&settings.cache.join("graphit-tsconfig.json"))?
    {
        return Ok(None);
    }
    let workspace = settings.workspace.to_str().ok_or(Error::Config)?;
    if workspace
        .len()
        .checked_mul(5)
        .and_then(|size| size.checked_add(256))
        .is_none_or(|size| size > MAX_CONFIG_BYTES)
    {
        return Err(Error::Limit);
    }
    let mut writer = ConfigWriter(Vec::new());
    serde_json::to_writer(&mut writer, &serde_json::json!({"compilerOptions":{"allowJs":true},"include":[format!("{workspace}/**/*")],"exclude":[format!("{workspace}/**/node_modules"),format!("{workspace}/**/.git"),format!("{workspace}/**/.graphit"),format!("{workspace}/**/.build")]})).map_err(|_| Error::Limit)?;
    Ok(Some(writer.0))
}

fn python_path(cache: &Path, path: &OsStr, limit: usize) -> Result<OsString, Error> {
    // Use the standard serializer for each component before admitting the total,
    // including native quotes and separators, then allocate the complete PATH.
    bounded_os(cache.as_os_str(), MAX_PATH_BYTES)?;
    bounded_os(path, MAX_PATH_BYTES)?;
    if limit == 0 {
        return Err(Error::Limit);
    }
    let bin = cache.join("venv/bin");
    let mut size = 0usize;
    for (index, part) in std::iter::once(bin.clone())
        .chain(std::env::split_paths(path))
        .enumerate()
    {
        let encoded = std::env::join_paths(std::iter::once(part)).map_err(|_| Error::Config)?;
        size = size
            .checked_add(usize::from(index != 0))
            .and_then(|size| size.checked_add(encoded.as_encoded_bytes().len()))
            .filter(|size| *size <= limit)
            .ok_or(Error::Limit)?;
    }
    let composed = std::env::join_paths(std::iter::once(bin).chain(std::env::split_paths(path)))
        .map_err(|_| Error::Config)?;
    bounded_os(&composed, limit)?;
    Ok(composed)
}

struct Runner<'a> {
    settings: &'a Settings,
    control: &'a Control,
    environment: Vec<(OsString, OsString)>,
}
impl Runner<'_> {
    fn env(&mut self, key: &str, value: impl AsRef<OsStr>) {
        self.environment
            .push((key.into(), value.as_ref().to_owned()));
    }
    fn cache_env(&mut self, key: &str, relative: &str) {
        self.env(key, self.settings.cache.join(relative));
    }
    fn command(&self, tool: &str) -> Command {
        let mut command = Command::new(tool);
        command
            .current_dir(&self.settings.workspace)
            .env("PATH", &self.settings.path)
            .envs(self.environment.iter().map(|(key, value)| (key, value)));
        command
    }
    fn run(&self, command: Command) -> Result<(), Error> {
        process::execute(command, self.control, false).map(|_| ())
    }
    fn write(&self, path: &Path, bytes: &[u8]) -> Result<(), Error> {
        self.control.check()?;
        if bytes.len() > MAX_CONFIG_BYTES {
            return Err(Error::Limit);
        }
        fs::write(path, bytes).map_err(|_| Error::Io)?;
        self.control.check()
    }
}

/// Execute a complete sequence. A successful upstream exit still requires a nonempty regular index.
pub fn run(settings: &Settings, control: &Control) -> Result<(), Error> {
    control.check()?;
    settings.validate()?;
    // Preflight owned inputs before removing the previous output or invoking vendors.
    let ts_config = if settings.family == Family::TypeScript {
        typescript_config(settings)?
    } else {
        None
    };
    let java_maven = if settings.family == Family::Java {
        let maven = settings.cache.join("maven");
        Some(format!(
            "-Dmaven.repo.local={}",
            maven.to_str().ok_or(Error::Config)?
        ))
    } else {
        None
    };
    let python_env = if settings.family == Family::Python {
        Some(python_path(
            &settings.cache,
            &settings.path,
            MAX_PATH_BYTES,
        )?)
    } else {
        None
    };
    let version = if settings.family == Family::Python {
        Some(python_version(settings, control)?)
    } else {
        None
    };
    let discovered = match settings.family {
        Family::Dotnet => candidates(
            &settings.workspace,
            &["sln", "slnx", "csproj", "vbproj"],
            true,
            control,
        )?,
        Family::Ruby => candidates(&settings.workspace, &["gemspec"], false, control)?,
        _ => Vec::new(),
    };
    if settings.family == Family::Dotnet && discovered.is_empty() {
        return Err(Error::Input);
    }
    if settings.family == Family::Php
        && (!regular(&settings.workspace.join("composer.json"))?
            || !regular(&settings.workspace.join("composer.lock"))?)
    {
        return Err(Error::Input);
    }
    if settings.family == Family::Clang
        && !regular(&settings.workspace.join("compile_commands.json"))?
        && !regular(&settings.workspace.join("build/compile_commands.json"))?
        && !regular(&settings.workspace.join("CMakeLists.txt"))?
    {
        return Err(Error::Input);
    }
    control.check()?;
    match fs::remove_file(&settings.output) {
        Ok(()) => (),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (),
        Err(_) => return Err(Error::Io),
    }
    let mut runner = Runner {
        settings,
        control,
        environment: Vec::new(),
    };
    let output = &settings.output;
    match settings.family {
        Family::Go => {
            runner.cache_env("GOPATH", "go");
            runner.cache_env("GOCACHE", "go-build");
            let mut command = runner.command("scip-go");
            command.args(["index", "--output"]).arg(output);
            runner.run(command)?;
        }
        Family::TypeScript => {
            runner.cache_env("npm_config_cache", "npm");
            runner.cache_env("COREPACK_HOME", "corepack");
            let mut command = runner.command("scip-typescript");
            command.arg("index");
            if !regular(&settings.workspace.join("tsconfig.json"))? {
                let config = settings.cache.join("graphit-tsconfig.json");
                if let Some(bytes) = ts_config {
                    runner.write(&config, &bytes)?;
                }
                command
                    .arg("--cwd")
                    .arg(&settings.workspace)
                    .arg("--output")
                    .arg(output)
                    .arg(config);
            } else {
                if regular(&settings.workspace.join("pnpm-lock.yaml"))? {
                    command.arg("--pnpm-workspaces");
                } else if regular(&settings.workspace.join("yarn.lock"))? {
                    command.arg("--yarn-workspaces");
                }
                command.arg("--output").arg(output);
            }
            runner.run(command)?;
        }
        Family::Python => {
            runner.cache_env("npm_config_cache", "npm");
            runner.cache_env("PIP_CACHE_DIR", "pip");
            let venv = settings.cache.join("venv");
            if !executable(&venv.join("bin/python3"))? {
                let mut command = runner.command("python3");
                command.args(["-m", "venv"]).arg(&venv);
                runner.run(command)?;
            }
            runner.env("PATH", python_env.ok_or(Error::Config)?);
            if regular(&settings.workspace.join("requirements.txt"))? {
                let mut command = runner.command("pip");
                command.args(["install", "-r", "requirements.txt"]);
                runner.run(command)?;
            }
            let mut command = runner.command("scip-python");
            command
                .arg("index")
                .arg("--cwd")
                .arg(&settings.workspace)
                .arg("--output")
                .arg(output)
                .arg(format!("--project-name={}", settings.project_name))
                .arg(format!(
                    "--project-version={}",
                    version.ok_or(Error::Input)?
                ));
            runner.run(command)?;
        }
        Family::Java => {
            runner.env("MAVEN_OPTS", java_maven.ok_or(Error::Config)?);
            runner.cache_env("GRADLE_USER_HOME", "gradle");
            let mut command = runner.command("scip-java");
            command
                .args(["index"])
                .arg(path_argument("--output=", output)?)
                .arg(path_argument(
                    "--targetroot=",
                    &settings.cache.join("java-target"),
                )?);
            runner.run(command)?;
        }
        Family::Clang => {
            let compdb = if regular(&settings.workspace.join("compile_commands.json"))? {
                PathBuf::from("compile_commands.json")
            } else if regular(&settings.workspace.join("build/compile_commands.json"))? {
                PathBuf::from("build/compile_commands.json")
            } else {
                let mut command = runner.command("cmake");
                command
                    .args(["-S", ".", "-B"])
                    .arg(settings.cache.join("cmake-build"))
                    .arg("-DCMAKE_EXPORT_COMPILE_COMMANDS=ON");
                runner.run(command)?;
                settings.cache.join("cmake-build/compile_commands.json")
            };
            let compdb = if let Some(root) = &settings.host_root {
                runner.env("GRAPHIT_SCIP_HOST_ROOT", root);
                let translated = settings.cache.join("graphit-compile-commands.json");
                let mut command = runner.command("graphit-filter-clang-compdb");
                command.arg(&compdb).arg(&translated);
                runner.run(command)?;
                translated
            } else {
                compdb
            };
            let mut command = runner.command("scip-clang");
            command
                .arg(path_argument("--compdb-path=", &compdb)?)
                .arg(path_argument("--index-output-path=", output)?);
            runner.run(command)?;
        }
        Family::Dotnet => {
            runner.cache_env("DOTNET_CLI_HOME", "dotnet");
            runner.cache_env("NUGET_PACKAGES", "nuget");
            let inputs = dotnet_inputs(&runner, discovered)?;
            for project in &inputs {
                let mut command = runner.command("dotnet");
                command
                    .arg("restore")
                    .arg(project)
                    .arg("/p:EnableWindowsTargeting=true");
                runner.run(command)?;
            }
            let mut command = runner.command("scip-dotnet");
            command
                .args(["index", "--output"])
                .arg(output)
                .arg("--skip-dotnet-restore")
                .args(inputs);
            runner.run(command)?;
        }
        Family::Rust => {
            runner.cache_env("CARGO_HOME", "cargo");
            runner.cache_env("CARGO_TARGET_DIR", "target");
            runner.env("RUSTUP_HOME", "/usr/local/rustup");
            let mut command = runner.command("rust-analyzer");
            command.args(["scip", ".", "--output"]).arg(output);
            runner.run(command)?;
        }
        Family::Ruby => {
            runner.cache_env("BUNDLE_PATH", "bundle");
            runner.cache_env("BUNDLE_APP_CONFIG", "bundle-config");
            let gemfile = settings.cache.join("scip-ruby.Gemfile");
            runner.env("BUNDLE_GEMFILE", &gemfile);
            let mut body = String::from("source \"https://rubygems.org\"\n");
            let source = settings.workspace.join("Gemfile");
            if regular(&source)? {
                runner.env("GRAPHIT_SCIP_SOURCE_GEMFILE", source);
                body.push_str("eval_gemfile ENV.fetch(\"GRAPHIT_SCIP_SOURCE_GEMFILE\")\n");
            }
            body.push_str("gem \"scip-ruby\", \"0.4.8\"\n");
            runner.write(&gemfile, body.as_bytes())?;
            let mut command = runner.command("bundle");
            command.arg("install");
            runner.run(command)?;
            let metadata = if let Some(gemspec) = discovered.first() {
                let mut command = runner.command("ruby");
                command.args(["-e", RUBY_METADATA]).arg(gemspec);
                let bytes = process::execute(command, control, true)?;
                #[derive(Deserialize)]
                struct Gem {
                    name: String,
                    version: String,
                }
                let gem: Option<Gem> = serde_json::from_slice(&bytes).map_err(|_| Error::Input)?;
                match gem {
                    Some(gem) => {
                        bounded_text(&gem.name)?;
                        bounded_text(&gem.version)?;
                        format!("{}@{}", gem.name, gem.version)
                    }
                    None => format!("{}@0.0.0", settings.project_name),
                }
            } else {
                format!("{}@0.0.0", settings.project_name)
            };
            bounded_text(&metadata)?;
            let mut command = runner.command("bundle");
            command
                .args(["exec", "scip-ruby", "--gem-metadata"])
                .arg(metadata)
                .arg("--index-file")
                .arg(output);
            if !regular(&settings.workspace.join("sorbet/config"))? {
                command.arg(".");
            }
            runner.run(command)?;
        }
        Family::Dart => {
            runner.cache_env("PUB_CACHE", "pub");
            if !settings
                .cache
                .join("pub/global_packages/scip_dart")
                .is_dir()
            {
                let mut command = runner.command("dart");
                command.args(["pub", "global", "activate", "scip_dart", "1.6.2"]);
                runner.run(command)?;
            }
            let mut command = runner.command("dart");
            command
                .args(["pub", "global", "run", "scip_dart", "--output"])
                .arg(output)
                .arg("./");
            runner.run(command)?;
        }
        Family::Php => {
            runner.cache_env("COMPOSER_HOME", "composer");
            runner.cache_env("COMPOSER_CACHE_DIR", "composer-cache");
            let mut command = runner.command("php");
            command
                .args(["-r", PHP_INDEX, "--"])
                .arg(&settings.php_autoload)
                .arg(&settings.workspace)
                .arg(output);
            runner.run(command)?;
        }
    }
    control.check()?;
    let metadata = fs::metadata(output).map_err(|_| Error::Io)?;
    if !metadata.is_file() || metadata.len() == 0 {
        return Err(Error::Io);
    }
    encoding::normalize(output, settings.family, control)?;
    control.check()
}
fn path_argument(prefix: &str, path: &Path) -> Result<OsString, Error> {
    bounded_os(path.as_os_str(), MAX_PATH_BYTES)?;
    let mut value = OsString::from(prefix);
    value.push(path);
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    #[test]
    fn serialized_python_path_caps_exact_one_over_zero_and_invalid_separator() {
        let cache = Path::new("/cache");
        let prefix = cache.join("venv/bin").as_os_str().as_encoded_bytes().len() + 1;
        for size in [MAX_PATH_BYTES - 1, MAX_PATH_BYTES] {
            let value = python_path(
                cache,
                OsStr::new(&"a".repeat(size - prefix)),
                MAX_PATH_BYTES,
            )
            .unwrap();
            assert_eq!(value.as_encoded_bytes().len(), size);
        }
        assert_eq!(
            python_path(
                cache,
                OsStr::new(&"a".repeat(MAX_PATH_BYTES + 1 - prefix)),
                MAX_PATH_BYTES
            ),
            Err(Error::Limit)
        );
        assert_eq!(python_path(cache, OsStr::new(""), 0), Err(Error::Limit));
        assert_eq!(
            python_path(
                Path::new("/cache:separator"),
                OsStr::new("/bin"),
                MAX_PATH_BYTES
            ),
            Err(Error::Config)
        );
    }
    #[cfg(windows)]
    #[test]
    fn serialized_python_path_uses_native_quotes_and_rejects_invalid_quotes() {
        let cache = Path::new(r"C:\cache;quoted");
        let value = python_path(cache, OsStr::new(r"C:\tools"), MAX_PATH_BYTES).unwrap();
        assert_eq!(
            std::env::split_paths(&value).next().unwrap(),
            cache.join("venv/bin")
        );
        assert_eq!(
            python_path(
                Path::new("C:\\cache\"invalid"),
                OsStr::new(r"C:\tools"),
                MAX_PATH_BYTES
            ),
            Err(Error::Config)
        );
        assert_eq!(
            python_path(cache, OsStr::new(r"C:\tools"), 0),
            Err(Error::Limit)
        );
    }
}
