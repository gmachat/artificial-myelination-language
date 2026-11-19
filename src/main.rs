use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{self, Command},
    time::{SystemTime, UNIX_EPOCH},
};

use aml_language::{
    agent, benchmark, c_backend, csharp, diagnose, external, format_source, fullstack, go, ir,
    java, javascript, json, lsp, mcp, modules, packages, python, react, ruby, rust_backend,
    scaffold, swift, test_runner, web, CompileError, REFERENCE, VERSION,
};

fn fail(message: impl AsRef<str>) -> ! {
    eprintln!("aml: {}", message.as_ref());
    process::exit(1);
}

fn json_quote(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('\r', "\\r")
            .replace('\t', "\\t")
    )
}

fn diagnostic_json(path: &Path, error: Option<&CompileError>) -> String {
    match error {
        None => "{\n  \"diagnostics\": [],\n  \"valid\": true\n}".into(),
        Some(error) => format!(
            "{{\n  \"diagnostics\": [\n    {{\n      \"code\": \"AML1000\",\n      \"column\": {},\n      \"line\": {},\n      \"message\": {},\n      \"path\": {},\n      \"severity\": \"error\"\n    }}\n  ],\n  \"valid\": false\n}}",
            error.column,
            error.line,
            json_quote(&error.message),
            json_quote(&path.display().to_string())
        ),
    }
}

fn reference_json() -> String {
    format!(
        "{{\n  \"indent\": 2,\n  \"reference\": {},\n  \"types\": [\"Any\", \"Bool\", \"Float\", \"Int\", \"Text\", \"List[T]\", \"Optional[T]\", \"Time\"],\n  \"version\": \"0.8\"\n}}",
        json_quote(REFERENCE)
    )
}

fn tool_found(command: &str, arguments: &[&str]) -> bool {
    Command::new(command)
        .args(arguments)
        .output()
        .is_ok_and(|output| output.status.success())
}

fn doctor(json: bool) {
    let tools = [
        ("javascript/react/web/fullstack", "node", vec!["--version"]),
        ("python", "python3", vec!["--version"]),
        ("ruby", "ruby", vec!["--version"]),
        ("go", "go", vec!["version"]),
        ("rust", "rustc", vec!["--version"]),
        ("c", "cc", vec!["--version"]),
        ("java", "javac", vec!["--version"]),
        ("csharp", "dotnet", vec!["--version"]),
        ("swift", "swiftc", vec!["--version"]),
    ];
    let results = tools
        .iter()
        .map(|(target, tool, arguments)| (*target, *tool, tool_found(tool, arguments)))
        .collect::<Vec<_>>();
    if json {
        let encoded = results
            .iter()
            .map(|(target, tool, found)| {
                format!(
                    "{{\"target\":{},\"tool\":{},\"found\":{}}}",
                    json_quote(target),
                    json_quote(tool),
                    found
                )
            })
            .collect::<Vec<_>>()
            .join(",");
        println!(
            "{{\"aml_version\":{},\"compiler\":\"ready\",\"tools\":[{}]}}",
            json_quote(VERSION),
            encoded
        );
        return;
    }
    println!("AML {VERSION}: compiler ready");
    for (target, tool, found) in results {
        println!(
            "{:<32} {:<10} {}",
            target,
            tool,
            if found { "found" } else { "not found" }
        );
    }
    println!("Tool detection confirms discovery only; use the conformance suite for execution certification.");
}

const TARGETS_JSON: &str = r#"{
  "c": {"native_rust": true, "packages": [], "platform": ["cc"]},
  "csharp": {"native_rust": true, "packages": [], "platform": ["dotnet8"]},
  "fullstack": {"native_rust": true, "packages": ["react", "vite"], "platform": ["node", "json-or-postgres"]},
  "go": {"native_rust": true, "packages": [], "platform": ["go"]},
  "ir": {"native_rust": true, "packages": [], "platform": []},
  "java": {"native_rust": true, "packages": [], "platform": ["jdk"]},
  "javascript": {"native_rust": true, "packages": [], "platform": ["node"]},
  "python": {"native_rust": true, "packages": [], "platform": ["python3"]},
  "react": {"native_rust": true, "packages": ["react"], "platform": ["node"]},
  "ruby": {"native_rust": true, "packages": [], "platform": ["ruby"]},
  "rust": {"native_rust": true, "packages": [], "platform": ["rustc"]},
  "swift": {"native_rust": true, "packages": [], "platform": ["swiftc"]},
  "web": {"native_rust": true, "packages": ["react", "vite"], "platform": ["node"]}
}"#;

fn option(args: &[String], name: &str) -> Option<String> {
    args.iter()
        .position(|value| value == name)
        .and_then(|index| args.get(index + 1))
        .cloned()
}

fn positional(args: &[String]) -> Option<&str> {
    let mut skip = false;
    for value in args {
        if skip {
            skip = false;
            continue;
        }
        if matches!(
            value.as_str(),
            "--target"
                | "-o"
                | "--output"
                | "--name"
                | "--project"
                | "--backend"
                | "--profile"
                | "--directory"
        ) {
            skip = true;
            continue;
        }
        if !value.starts_with('-') {
            return Some(value);
        }
    }
    None
}

fn all_positionals(args: &[String]) -> Vec<&str> {
    let mut output = Vec::new();
    let mut skip = false;
    for value in args {
        if skip {
            skip = false;
            continue;
        }
        if matches!(
            value.as_str(),
            "--target"
                | "-o"
                | "--output"
                | "--name"
                | "--project"
                | "--backend"
                | "--profile"
                | "--directory"
        ) {
            skip = true;
            continue;
        }
        if !value.starts_with('-') {
            output.push(value.as_str());
        }
    }
    output
}

fn read(path: &Path) -> String {
    fs::read_to_string(path)
        .unwrap_or_else(|error| fail(format!("cannot read {}: {error}", path.display())))
}

fn write_project(output: &Path, files: std::collections::BTreeMap<String, String>, force: bool) {
    if output.is_file() {
        fail(format!(
            "web output exists and is not a directory: {}",
            output.display()
        ));
    }
    if output.is_dir()
        && !force
        && fs::read_dir(output).is_ok_and(|mut entries| entries.next().is_some())
    {
        fail(format!(
            "web output directory is not empty: {}; use --force to update it",
            output.display()
        ));
    }
    for (relative, content) in files {
        let destination = output.join(relative);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).unwrap_or_else(|error| fail(error.to_string()));
        }
        fs::write(&destination, content).unwrap_or_else(|error| {
            fail(format!("cannot write {}: {error}", destination.display()))
        });
    }
}

fn usage() -> ! {
    fail("usage: aml <check|build|run|test|diagnose|format|reference|benchmark|doctor|agent|new|targets|backend-check|lsp|mcp|init|add|lock> [options]")
}

fn run_generated(runtime: &str, label: &str, extension: &str, source: &str) -> ! {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |value| value.as_nanos());
    let path = env::temp_dir().join(format!("aml-run-{}-{nonce}.{extension}", process::id()));
    fs::write(&path, source).unwrap_or_else(|error| fail(error.to_string()));
    let status = Command::new(runtime).arg(&path).status();
    let _ = fs::remove_file(path);
    match status {
        Ok(status) => process::exit(status.code().unwrap_or(1)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fail(format!("{label} runtime is required"))
        }
        Err(error) => fail(error.to_string()),
    }
}

fn run_go(source: &str) -> ! {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |value| value.as_nanos());
    let root = env::temp_dir().join(format!("aml-go-{}-{nonce}", process::id()));
    fs::create_dir_all(&root).unwrap_or_else(|error| fail(error.to_string()));
    let path = root.join("main.go");
    fs::write(&path, source).unwrap_or_else(|error| fail(error.to_string()));
    let status = Command::new("go")
        .arg("run")
        .arg(&path)
        .env("GOCACHE", root.join("cache"))
        .status();
    let _ = fs::remove_dir_all(root);
    match status {
        Ok(status) => process::exit(status.code().unwrap_or(1)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fail("Go runtime is required")
        }
        Err(error) => fail(error.to_string()),
    }
}

fn run_rust(source: &str) -> ! {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |value| value.as_nanos());
    let root = env::temp_dir().join(format!("aml-rust-{}-{nonce}", process::id()));
    fs::create_dir_all(&root).unwrap_or_else(|error| fail(error.to_string()));
    let path = root.join("main.rs");
    let executable = root.join("program");
    fs::write(&path, source).unwrap_or_else(|error| fail(error.to_string()));
    let compiled = Command::new("rustc")
        .arg(&path)
        .arg("-o")
        .arg(&executable)
        .status();
    let status = match compiled {
        Ok(status) if status.success() => Command::new(&executable).status(),
        Ok(status) => {
            let _ = fs::remove_dir_all(root);
            process::exit(status.code().unwrap_or(1));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let _ = fs::remove_dir_all(root);
            fail("Rust toolchain is required");
        }
        Err(error) => {
            let _ = fs::remove_dir_all(root);
            fail(error.to_string());
        }
    };
    let _ = fs::remove_dir_all(root);
    match status {
        Ok(status) => process::exit(status.code().unwrap_or(1)),
        Err(error) => fail(error.to_string()),
    }
}

fn run_c(source: &str) -> ! {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |value| value.as_nanos());
    let root = env::temp_dir().join(format!("aml-c-{}-{nonce}", process::id()));
    fs::create_dir_all(&root).unwrap_or_else(|error| fail(error.to_string()));
    let path = root.join("main.c");
    let executable = root.join("program");
    fs::write(&path, source).unwrap_or_else(|error| fail(error.to_string()));
    let compiled = Command::new("cc")
        .args(["-std=c11"])
        .arg(&path)
        .arg("-o")
        .arg(&executable)
        .status();
    let status = match compiled {
        Ok(status) if status.success() => Command::new(&executable).status(),
        Ok(status) => {
            let _ = fs::remove_dir_all(root);
            process::exit(status.code().unwrap_or(1));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let _ = fs::remove_dir_all(root);
            fail("C compiler is required");
        }
        Err(error) => {
            let _ = fs::remove_dir_all(root);
            fail(error.to_string());
        }
    };
    let _ = fs::remove_dir_all(root);
    match status {
        Ok(status) => process::exit(status.code().unwrap_or(1)),
        Err(error) => fail(error.to_string()),
    }
}

fn run_java(source: &str) -> ! {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |value| value.as_nanos());
    let root = env::temp_dir().join(format!("aml-java-{}-{nonce}", process::id()));
    fs::create_dir_all(&root).unwrap_or_else(|error| fail(error.to_string()));
    let path = root.join("AMLProgram.java");
    fs::write(&path, source).unwrap_or_else(|error| fail(error.to_string()));
    let compiled = Command::new("javac").arg(&path).status();
    let status = match compiled {
        Ok(status) if status.success() => Command::new("java")
            .args(["-cp", root.to_str().unwrap(), "AMLProgram"])
            .status(),
        Ok(status) => {
            let _ = fs::remove_dir_all(root);
            process::exit(status.code().unwrap_or(1));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let _ = fs::remove_dir_all(root);
            fail("Java JDK is required");
        }
        Err(error) => {
            let _ = fs::remove_dir_all(root);
            fail(error.to_string());
        }
    };
    let _ = fs::remove_dir_all(root);
    match status {
        Ok(status) => process::exit(status.code().unwrap_or(1)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => fail("Java JDK is required"),
        Err(error) => fail(error.to_string()),
    }
}

fn run_csharp(source: &str) -> ! {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |value| value.as_nanos());
    let root = env::temp_dir().join(format!("aml-csharp-{}-{nonce}", process::id()));
    fs::create_dir_all(&root).unwrap_or_else(|error| fail(error.to_string()));
    fs::write(root.join("Program.cs"), source).unwrap_or_else(|error| fail(error.to_string()));
    fs::write(
        root.join("AMLProgram.csproj"),
        "<Project Sdk=\"Microsoft.NET.Sdk\"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><ImplicitUsings>disable</ImplicitUsings><Nullable>enable</Nullable></PropertyGroup></Project>\n",
    )
    .unwrap_or_else(|error| fail(error.to_string()));
    let status = Command::new("dotnet")
        .arg("run")
        .arg("--project")
        .arg(root.join("AMLProgram.csproj"))
        .arg("--nologo")
        .status();
    let _ = fs::remove_dir_all(root);
    match status {
        Ok(status) => process::exit(status.code().unwrap_or(1)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fail(".NET 8 SDK is required")
        }
        Err(error) => fail(error.to_string()),
    }
}

fn run_swift(source: &str) -> ! {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |value| value.as_nanos());
    let root = env::temp_dir().join(format!("aml-swift-{}-{nonce}", process::id()));
    fs::create_dir_all(&root).unwrap_or_else(|error| fail(error.to_string()));
    let path = root.join("main.swift");
    let executable = root.join("program");
    fs::write(&path, source).unwrap_or_else(|error| fail(error.to_string()));
    let compiled = Command::new("swiftc")
        .arg(&path)
        .arg("-o")
        .arg(&executable)
        .env("CLANG_MODULE_CACHE_PATH", root.join("cache"))
        .status();
    let status = match compiled {
        Ok(status) if status.success() => Command::new(&executable).status(),
        Ok(status) => {
            let _ = fs::remove_dir_all(root);
            process::exit(status.code().unwrap_or(1));
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let _ = fs::remove_dir_all(root);
            fail("Swift toolchain is required");
        }
        Err(error) => {
            let _ = fs::remove_dir_all(root);
            fail(error.to_string());
        }
    };
    let _ = fs::remove_dir_all(root);
    match status {
        Ok(status) => process::exit(status.code().unwrap_or(1)),
        Err(error) => fail(error.to_string()),
    }
}

fn main() {
    let mut values: Vec<String> = env::args().skip(1).collect();
    if values.is_empty() {
        usage();
    }
    if values[0] == "--version" || values[0] == "-V" {
        println!("AML {VERSION}");
        return;
    }
    let command = values.remove(0);
    match command.as_str() {
        "mcp" => {
            let project = option(&values, "--project").unwrap_or_else(|| ".".into());
            if values.iter().any(|value| value == "--print-config") {
                print!(
                    "{}",
                    mcp::configuration(Path::new(&project)).unwrap_or_else(|error| fail(error))
                );
            } else {
                mcp::serve(Path::new(&project))
                    .unwrap_or_else(|error| fail(format!("MCP server: {error}")));
            }
        }
        "agent" => match values.first().map(String::as_str) {
            Some("context") => {
                let project = option(&values, "--project").unwrap_or_else(|| ".".into());
                print!(
                    "{}",
                    agent::context(Path::new(&project)).unwrap_or_else(|error| fail(error))
                );
            }
            Some("repair-context") => {
                let arguments = all_positionals(&values);
                let Some(file) = arguments.get(1) else {
                    fail("usage: aml agent repair-context <file> [--target TARGET]");
                };
                let target = option(&values, "--target").unwrap_or_else(|| "javascript".into());
                print!(
                    "{}",
                    diagnose::report(Path::new(file), &target).unwrap_or_else(|error| fail(error))
                );
            }
            _ => fail("usage: aml agent <context|repair-context> [options]"),
        },
        "new" => {
            let Some(name) = positional(&values) else {
                fail("usage: aml new <name> [--profile script|react|web|fullstack] [--directory DIR]");
            };
            let profile = option(&values, "--profile").unwrap_or_else(|| "script".into());
            let directory = option(&values, "--directory")
                .map(PathBuf::from)
                .unwrap_or_else(|| PathBuf::from(name));
            match scaffold::create(&directory, name, &profile) {
                Ok(paths) => {
                    for path in paths {
                        println!("{}", path.display());
                    }
                    println!(
                        "next: cd {} && aml check main.aml --json",
                        directory.display()
                    );
                }
                Err(error) => fail(error),
            }
        }
        "doctor" => doctor(values.iter().any(|value| value == "--json")),
        "diagnose" => {
            let Some(file) = positional(&values) else {
                fail("usage: aml diagnose <file> [--target TARGET] [--json]");
            };
            let target = option(&values, "--target").unwrap_or_else(|| "javascript".into());
            print!(
                "{}",
                diagnose::report(Path::new(file), &target).unwrap_or_else(|error| fail(error))
            );
        }
        "test" => {
            let directory = positional(&values).unwrap_or("tests");
            let target = option(&values, "--target").unwrap_or_else(|| "javascript".into());
            let report =
                test_runner::run(Path::new(directory), &target).unwrap_or_else(|error| fail(error));
            print!("{}", report.json);
            if report.failed > 0 {
                process::exit(1);
            }
        }
        "reference" => {
            if values.iter().any(|value| value == "--json") {
                println!("{}", reference_json());
            } else {
                print!("{REFERENCE}");
            }
        }
        "targets" => {
            if values.iter().any(|value| value == "--json") {
                println!("{TARGETS_JSON}");
                return;
            }
            println!("ir: native-rust=yes; format=typed-json-1.1");
            println!("javascript: native-rust=yes; platform=node");
            println!("ruby: native-rust=yes; platform=ruby");
            println!("go: native-rust=yes; platform=go");
            println!("rust: native-rust=yes; platform=rustc");
            println!("c: native-rust=yes; platform=c11");
            println!("java: native-rust=yes; platform=jdk");
            println!("csharp: native-rust=yes; platform=dotnet8");
            println!("swift: native-rust=yes; platform=swiftc");
            println!("react: native-rust=yes; packages=react");
            println!("web: native-rust=yes; packages=react,vite");
            println!("fullstack: native-rust=yes; packages=react,vite,node; storage=json,postgres");
            println!("python: native-rust=yes; platform=python3");
        }
        "benchmark" => {
            let language = option(&values, "--language").unwrap_or_else(|| "all".into());
            let json = values.iter().any(|value| value == "--json");
            print!(
                "{}",
                benchmark::report(&language, json).unwrap_or_else(|error| fail(error))
            );
        }
        "backend-check" => {
            let Some(executable) = positional(&values) else {
                fail("usage: aml backend-check <executable> [--json]");
            };
            let executable = Path::new(executable)
                .canonicalize()
                .unwrap_or_else(|error| fail(format!("cannot resolve backend: {error}")));
            match external::check(&executable) {
                Ok(report) if values.iter().any(|value| value == "--json") => {
                    print!("{}", json::pretty(&report))
                }
                Ok(report) => {
                    let cases = report
                        .object()
                        .ok()
                        .and_then(|root| root.get("cases"))
                        .and_then(|value| match value {
                            json::Value::Array(values) => Some(values.len()),
                            _ => None,
                        })
                        .unwrap_or(0);
                    println!("ok: {} ({cases} protocol fixtures)", executable.display());
                }
                Err(error) => fail(error),
            }
        }
        "lsp" => {
            lsp::serve_stdio().unwrap_or_else(|error| fail(format!("language server: {error}")))
        }
        "init" => {
            let directory = positional(&values).map_or_else(|| PathBuf::from("."), PathBuf::from);
            let name = option(&values, "--name");
            match packages::init(&directory, name.as_deref()) {
                Ok(paths) => {
                    for path in paths {
                        println!("{}", path.display());
                    }
                }
                Err(error) => fail(error),
            }
        }
        "add" => {
            let arguments = all_positionals(&values);
            if arguments.len() != 2 {
                fail("usage: aml add <name> <path> [--project DIR]");
            }
            let project = option(&values, "--project").unwrap_or_else(|| ".".into());
            match packages::add(Path::new(&project), arguments[0], Path::new(arguments[1])) {
                Ok(path) => {
                    println!("{}", path.display());
                    match packages::lock(Path::new(&project)) {
                        Ok(path) => println!("{}", path.display()),
                        Err(error) => fail(error),
                    }
                }
                Err(error) => fail(error),
            }
        }
        "lock" => {
            let project = option(&values, "--project").unwrap_or_else(|| ".".into());
            match packages::lock(Path::new(&project)) {
                Ok(path) => println!("{}", path.display()),
                Err(error) => fail(error),
            }
        }
        "format" => {
            let check = values.iter().any(|value| value == "--check");
            let Some(file) = positional(&values) else {
                usage();
            };
            let path = Path::new(file);
            let source = read(path);
            let formatted = format_source(&source);
            if check {
                if source != formatted {
                    process::exit(1);
                }
            } else if source != formatted {
                fs::write(path, formatted).unwrap_or_else(|error| fail(error.to_string()));
                println!("{}", path.display());
            }
        }
        "check" | "build" | "run" => {
            let Some(file) = positional(&values) else {
                usage();
            };
            let path = Path::new(file);
            let target = option(&values, "--target").unwrap_or_else(|| "javascript".into());
            let json_output = values.iter().any(|value| value == "--json");
            let source_result: Result<String, CompileError> = if matches!(
                target.as_str(),
                "javascript"
                    | "js"
                    | "python"
                    | "ruby"
                    | "go"
                    | "rust"
                    | "c"
                    | "java"
                    | "csharp"
                    | "cs"
                    | "swift"
                    | "ir"
            ) {
                modules::load_program(path)
            } else {
                Ok(read(path))
            };
            let source = match source_result {
                Ok(source) => source,
                Err(error) if command == "check" && json_output => {
                    println!("{}", diagnostic_json(path, Some(&error)));
                    process::exit(1);
                }
                Err(error) => fail(format!("{}:{error}", path.display())),
            };
            if command == "build" {
                if let Some(backend) = option(&values, "--backend") {
                    let backend = Path::new(&backend)
                        .canonicalize()
                        .unwrap_or_else(|error| fail(format!("cannot resolve backend: {error}")));
                    let ir_source = ir::compile_source(&source, &path.display().to_string())
                        .unwrap_or_else(|error| fail(format!("{}:{error}", path.display())));
                    let typed_ir = json::parse(&ir_source)
                        .unwrap_or_else(|error| fail(format!("cannot encode AML IR: {error}")));
                    let result = external::run(
                        &backend,
                        typed_ir,
                        &path.display().to_string(),
                        std::time::Duration::from_secs(30),
                    )
                    .unwrap_or_else(|error| fail(error));
                    let output = option(&values, "-o")
                        .or_else(|| option(&values, "--output"))
                        .map(PathBuf::from)
                        .unwrap_or_else(|| {
                            PathBuf::from(format!(
                                "{}-backend",
                                path.file_stem().unwrap().to_string_lossy()
                            ))
                        });
                    let force = values.iter().any(|value| value == "--force");
                    if output.is_file() {
                        fail(format!(
                            "backend output exists and is not a directory: {}",
                            output.display()
                        ));
                    }
                    if output.is_dir()
                        && !force
                        && fs::read_dir(&output).is_ok_and(|mut entries| entries.next().is_some())
                    {
                        fail(format!(
                            "backend output directory is not empty: {}; use --force to update it",
                            output.display()
                        ));
                    }
                    external::write_result(&result, &output).unwrap_or_else(|error| fail(error));
                    println!("{}", output.display());
                    return;
                }
            }
            let result = match target.as_str() {
                "react" => react::compile(&source).map(|generated| (Some(generated), None)),
                "web" => web::project_files(&source).map(|files| (None, Some(files))),
                "fullstack" => fullstack::project_files(&source).map(|files| (None, Some(files))),
                "javascript" | "js" => {
                    javascript::compile_source(&source).map(|generated| (Some(generated), None))
                }
                "python" => python::compile_source(&source)
                    .map(|generated| (Some(generated), None)),
                "ruby" => ruby::compile_source(&source)
                    .map(|generated| (Some(generated), None)),
                "go" => go::compile_source(&source)
                    .map(|generated| (Some(generated), None)),
                "rust" => rust_backend::compile_source(&source)
                    .map(|generated| (Some(generated), None)),
                "c" => c_backend::compile_source(&source)
                    .map(|generated| (Some(generated), None)),
                "java" => java::compile_source(&source)
                    .map(|generated| (Some(generated), None)),
                "csharp" | "cs" => csharp::compile_source(&source)
                    .map(|generated| (Some(generated), None)),
                "swift" => swift::compile_source(&source)
                    .map(|generated| (Some(generated), None)),
                "ir" => ir::compile_source(&source, &path.display().to_string())
                    .map(|generated| (Some(generated), None)),
                _ => fail(format!("target '{target}' has not moved to the Rust compiler yet; use the Python 0.8 compatibility compiler")),
            };
            let (generated, project) = match result {
                Ok(value) => value,
                Err(error) if command == "check" && json_output => {
                    println!("{}", diagnostic_json(path, Some(&error)));
                    process::exit(1);
                }
                Err(error) => fail(format!("{}:{error}", path.display())),
            };
            if command == "check" {
                if json_output {
                    println!("{}", diagnostic_json(path, None));
                } else {
                    println!("ok: {}", path.display());
                }
                return;
            }
            if command == "run" {
                match target.as_str() {
                    "javascript" | "js" => {
                        run_generated("node", "Node.js", "js", generated.as_deref().unwrap())
                    }
                    "python" => {
                        run_generated("python3", "Python 3", "py", generated.as_deref().unwrap())
                    }
                    "ruby" => run_generated("ruby", "Ruby", "rb", generated.as_deref().unwrap()),
                    "go" => run_go(generated.as_deref().unwrap()),
                    "rust" => run_rust(generated.as_deref().unwrap()),
                    "c" => run_c(generated.as_deref().unwrap()),
                    "java" => run_java(generated.as_deref().unwrap()),
                    "csharp" | "cs" => run_csharp(generated.as_deref().unwrap()),
                    "swift" => run_swift(generated.as_deref().unwrap()),
                    _ => fail("this target cannot run directly"),
                }
            }
            let force = values.iter().any(|value| value == "--force");
            let output = option(&values, "-o")
                .or_else(|| option(&values, "--output"))
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    if target == "react" {
                        path.with_extension("jsx")
                    } else if target == "ir" {
                        path.with_extension("amlir.json")
                    } else if matches!(target.as_str(), "javascript" | "js") {
                        path.with_extension("js")
                    } else if target == "python" {
                        path.with_extension("py")
                    } else if target == "ruby" {
                        path.with_extension("rb")
                    } else if target == "go" {
                        path.with_extension("go")
                    } else if target == "rust" {
                        path.with_extension("rs")
                    } else if target == "c" {
                        path.with_extension("c")
                    } else if target == "java" {
                        path.with_extension("java")
                    } else if matches!(target.as_str(), "csharp" | "cs") {
                        path.with_extension("cs")
                    } else if target == "swift" {
                        path.with_extension("swift")
                    } else if target == "fullstack" {
                        PathBuf::from(format!(
                            "{}-fullstack",
                            path.file_stem().unwrap().to_string_lossy()
                        ))
                    } else {
                        PathBuf::from(format!(
                            "{}-web",
                            path.file_stem().unwrap().to_string_lossy()
                        ))
                    }
                });
            if let Some(content) = generated {
                if let Some(parent) = output.parent() {
                    fs::create_dir_all(parent).unwrap_or_else(|error| fail(error.to_string()));
                }
                fs::write(&output, content).unwrap_or_else(|error| fail(error.to_string()));
            } else {
                write_project(&output, project.unwrap(), force);
            }
            println!("{}", output.display());
        }
        _ => usage(),
    }
}
