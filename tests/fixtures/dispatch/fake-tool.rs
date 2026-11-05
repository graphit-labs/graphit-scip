// A real subprocess fixture. This is never copied into production images.
use std::{env, fs, io::Write, path::PathBuf, process::Command, thread, time::Duration};
fn quote(value: &str) -> String {
    let mut text = String::from("\"");
    for character in value.chars() {
        match character { '"' => text.push_str("\\\""), '\\' => text.push_str("\\\\"), '\n' => text.push_str("\\n"), '\r' => text.push_str("\\r"), '\t' => text.push_str("\\t"), c if c.is_control() => text.push_str(&format!("\\u{:04x}", c as u32)), c => text.push(c) }
    }
    text.push('"'); text
}
fn main() {
    // Native argv can contain opaque Unix bytes. Display them lossily in JSON,
    // while current_dir/current_exe retain OS paths for the fixture's own I/O.
    let args: Vec<_> = env::args_os().map(|value| value.to_string_lossy().into_owned()).collect();
    if args.get(1).map(String::as_str) == Some("--fixture-install") {
        let source = env::current_exe().unwrap();
        let directory = PathBuf::from(&args[2]);
        for name in &args[3..] { fs::copy(&source, directory.join(format!("{name}{}", env::consts::EXE_SUFFIX))).unwrap(); }
        return;
    }
    if args.get(1).map(String::as_str) == Some("--fixture-descendant") {
        let pidfile = args.get(2).unwrap(); fs::write(pidfile, std::process::id().to_string()).unwrap();
        loop { thread::sleep(Duration::from_secs(60)); }
    }
    let invocation = PathBuf::from(&args[0]);
    let filename = invocation.file_name().unwrap().to_str().unwrap();
    let tool = filename.strip_suffix(env::consts::EXE_SUFFIX).unwrap_or(filename);
    let executable = env::current_exe().unwrap();
    let root = executable.ancestors().find(|ancestor| ancestor.join("bin/.fixture-root").is_file()).map(|ancestor| ancestor.join("bin")).unwrap_or_else(|| executable.parent().unwrap().to_owned());
    let options = fs::read_to_string(root.join(format!("{tool}.mode"))).unwrap_or_default();
    let option = |name: &str| options.lines().find_map(|line| line.strip_prefix(&format!("{name}=")));
    let keys = ["PATH", "GOPATH", "GOCACHE", "npm_config_cache", "COREPACK_HOME", "PIP_CACHE_DIR", "MAVEN_OPTS", "GRADLE_USER_HOME", "GRAPHIT_SCIP_HOST_ROOT", "DOTNET_CLI_HOME", "NUGET_PACKAGES", "CARGO_HOME", "CARGO_TARGET_DIR", "RUSTUP_HOME", "BUNDLE_PATH", "BUNDLE_APP_CONFIG", "BUNDLE_GEMFILE", "GRAPHIT_SCIP_SOURCE_GEMFILE", "PUB_CACHE", "COMPOSER_HOME", "COMPOSER_CACHE_DIR"];
    let record = format!("{{\"tool\":{},\"args\":[{}],\"cwd\":{},\"env\":{{{}}}}}\n", quote(tool), args[1..].iter().map(|value| quote(value)).collect::<Vec<_>>().join(","), quote(&env::current_dir().unwrap().to_string_lossy()), keys.iter().filter_map(|key| env::var(key).ok().map(|value| format!("{}:{}", quote(key), quote(&value)))).collect::<Vec<_>>().join(","));
    fs::OpenOptions::new().create(true).append(true).open(root.join("calls.jsonl")).unwrap().write_all(record.as_bytes()).unwrap();
    if option("hold") == Some("true") {
        fs::write(root.join("hold-ready"), "").unwrap();
        loop { thread::park(); }
    }
    if let Some(pidfile) = option("descendant") {
        Command::new(env::current_exe().unwrap()).args(["--fixture-descendant", pidfile]).spawn().unwrap();
        while !std::path::Path::new(pidfile).exists() { thread::sleep(Duration::from_millis(1)); }
    }
    if let Some(delay) = option("sleep") { thread::sleep(Duration::from_millis(delay.parse().unwrap())); }
    if let Some(code) = option("exit") { std::process::exit(code.parse().unwrap()); }
    if tool == "dotnet" && args.get(1).map(String::as_str) == Some("sln") {
        println!("Project(s)\n----------");
        if let Some(count) = option("huge") { print!("{}", "a".repeat(count.parse().unwrap())); }
        else if let Some(projects) = option("projects") { for project in projects.split('|') { println!("{project}"); } }
        return;
    }
    if tool == "ruby" {
        if let Some(count) = option("huge") { print!("{}", "a".repeat(count.parse().unwrap())); }
        else if !args.get(2).is_some_and(|code| code.contains("JSON.generate")) {
            if fs::read_dir(env::current_dir().unwrap()).unwrap().any(|entry|entry.unwrap().path().extension().is_some_and(|extension|extension=="gemspec")) { print!("fixture@1.2.3"); }
        }
        else { print!("{}", option("metadata").unwrap_or("{\"name\":\"fixture\",\"version\":\"1.2.3\"}")); }
        return;
    }
    if tool == "python3" && args.get(1).map(String::as_str) == Some("-c") { print!("0.0.0\n"); return; }
    if tool == "python3" && args.get(1).map(String::as_str) == Some("-m") {
        let bin = PathBuf::from(&args[3]).join(if cfg!(windows) { "Scripts" } else { "bin" }); fs::create_dir_all(&bin).unwrap();
        for name in [if cfg!(windows) { "python" } else { "python3" }, "pip"] { fs::copy(env::current_exe().unwrap(), bin.join(format!("{name}{}", env::consts::EXE_SUFFIX))).unwrap(); }
        return;
    }
    if tool == "cmake" {
        let directory = PathBuf::from(args.get(args.iter().position(|argument| argument == "-B").unwrap()+1).unwrap());
        fs::create_dir_all(&directory).unwrap(); fs::write(directory.join("compile_commands.json"), "[]").unwrap(); return;
    }
    if tool == "graphit-filter-clang-compdb" { fs::write(&args[2], "[]").unwrap(); return; }
    if tool == "dart" && args.iter().any(|argument| argument == "activate") {
        fs::create_dir_all(PathBuf::from(env::var("PUB_CACHE").unwrap()).join("global_packages/scip_dart")).unwrap(); return;
    }
    let output = if tool == "php" { if args.len() == 3 { Some("/output/index.scip".into()) } else { args.last().cloned() } }
        else { args.iter().enumerate().find_map(|(index, value)| {
            if matches!(value.as_str(), "--output" | "--index-file") { args.get(index+1).cloned() }
            else { value.strip_prefix("--output=").or_else(|| value.strip_prefix("--index-output-path=")).map(str::to_owned) }
        }) };
    if let Some(output) = output {
        // Official Index: metadata + one Document with explicit UTF8 positions.
        // This fixture tests subprocess orchestration, not vendor semantics.
        let index = b"\x0a\x13\x1a\x11file:///workspace\x12\x0d\x0a\x09fixture.x\x30\x01";
        if option("output") != Some("missing") { fs::write(output, if option("output") == Some("empty") { b"".as_slice() } else if option("output") == Some("malformed") { b"fixture-scip".as_slice() } else { index.as_slice() }).unwrap(); }
    }
}
