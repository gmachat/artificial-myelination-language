use std::{
    collections::BTreeMap,
    env, fs,
    io::{self, BufRead, Write},
    path::{Component, Path, PathBuf},
    process::Command,
};

use crate::{
    agent, format_source,
    json::{self, Value},
    VERSION,
};

const MAX_MESSAGE_BYTES: usize = 4 * 1024 * 1024;
const MODERN_PROTOCOL: &str = "2026-07-28";
const LEGACY_PROTOCOL: &str = "2025-11-25";

const TOOL_CATALOG: &str = r#"[
  {
    "name":"aml_project_context",
    "description":"Return a compact, deterministic AML project context packet for an AI coding agent.",
    "inputSchema":{"type":"object","properties":{},"additionalProperties":false},
    "annotations":{"readOnlyHint":true,"idempotentHint":true,"openWorldHint":false}
  },
  {
    "name":"aml_check",
    "description":"Check an AML source file and return structured compiler diagnostics.",
    "inputSchema":{"type":"object","properties":{"path":{"type":"string","description":"Project-relative AML file path."},"target":{"type":"string","enum":["javascript","python","ruby","go","rust","c","java","csharp","swift","react","web","fullstack","ir"]}},"required":["path"],"additionalProperties":false},
    "annotations":{"readOnlyHint":true,"idempotentHint":true,"openWorldHint":false}
  },
  {
    "name":"aml_format",
    "description":"Canonically format one AML source file inside the project root.",
    "inputSchema":{"type":"object","properties":{"path":{"type":"string","description":"Project-relative AML file path."}},"required":["path"],"additionalProperties":false},
    "annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}
  },
  {
    "name":"aml_build",
    "description":"Compile an AML source file to a project-relative output file or directory.",
    "inputSchema":{"type":"object","properties":{"path":{"type":"string"},"target":{"type":"string","enum":["javascript","python","ruby","go","rust","c","java","csharp","swift","react","web","fullstack","ir"]},"output":{"type":"string"},"force":{"type":"boolean","default":false}},"required":["path","target","output"],"additionalProperties":false},
    "annotations":{"readOnlyHint":false,"destructiveHint":false,"idempotentHint":true,"openWorldHint":false}
  },
  {
    "name":"aml_diagnose",
    "description":"Return a compact repair packet with categorized diagnostics and a small source window.",
    "inputSchema":{"type":"object","properties":{"path":{"type":"string"},"target":{"type":"string","enum":["javascript","python","ruby","go","rust","c","java","csharp","swift","react","web","fullstack","ir"]}},"required":["path"],"additionalProperties":false},
    "annotations":{"readOnlyHint":true,"idempotentHint":true,"openWorldHint":false}
  },
  {
    "name":"aml_repair_context",
    "description":"Return the minimal structured context needed for an AI to repair one AML file.",
    "inputSchema":{"type":"object","properties":{"path":{"type":"string"},"target":{"type":"string","enum":["javascript","python","ruby","go","rust","c","java","csharp","swift","react","web","fullstack","ir"]}},"required":["path"],"additionalProperties":false},
    "annotations":{"readOnlyHint":true,"idempotentHint":true,"openWorldHint":false}
  },
  {
    "name":"aml_test",
    "description":"Discover and run AML test programs beneath a project-relative directory.",
    "inputSchema":{"type":"object","properties":{"directory":{"type":"string","default":"tests"},"target":{"type":"string","default":"javascript","enum":["javascript","python","ruby","go","rust","c","java","csharp","swift"]}},"additionalProperties":false},
    "annotations":{"readOnlyHint":true,"idempotentHint":false,"openWorldHint":false}
  },
  {
    "name":"aml_doctor",
    "description":"Report which optional target toolchains AML can discover on this machine.",
    "inputSchema":{"type":"object","properties":{},"additionalProperties":false},
    "annotations":{"readOnlyHint":true,"idempotentHint":true,"openWorldHint":false}
  },
  {
    "name":"aml_targets",
    "description":"List AML compilation targets and their platform requirements.",
    "inputSchema":{"type":"object","properties":{},"additionalProperties":false},
    "annotations":{"readOnlyHint":true,"idempotentHint":true,"openWorldHint":false}
  }
]"#;

fn object(entries: impl IntoIterator<Item = (impl Into<String>, Value)>) -> Value {
    Value::Object(
        entries
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
    )
}

fn server_info() -> Value {
    object([
        ("name", Value::String("aml-language".into())),
        ("version", Value::String(VERSION.into())),
    ])
}

fn capabilities() -> Value {
    object([("tools", object([("listChanged", Value::Bool(false))]))])
}

fn result(id: Value, mut value: Value) -> Value {
    if let Value::Object(fields) = &mut value {
        fields
            .entry("_meta".into())
            .or_insert_with(|| object([("io.modelcontextprotocol/serverInfo", server_info())]));
    }
    object([
        ("jsonrpc", Value::String("2.0".into())),
        ("id", id),
        ("result", value),
    ])
}

fn error(id: Value, code: i64, message: impl Into<String>) -> Value {
    object([
        ("jsonrpc", Value::String("2.0".into())),
        ("id", id),
        (
            "error",
            object([
                ("code", Value::Number(code.to_string())),
                ("message", Value::String(message.into())),
            ]),
        ),
    ])
}

fn text_result(text: String, is_error: bool) -> Value {
    object([
        ("resultType", Value::String("complete".into())),
        (
            "content",
            Value::Array(vec![object([
                ("type", Value::String("text".into())),
                ("text", Value::String(text)),
            ])]),
        ),
        ("isError", Value::Bool(is_error)),
    ])
}

fn argument<'a>(arguments: &'a BTreeMap<String, Value>, name: &str) -> Result<&'a Value, String> {
    arguments
        .get(name)
        .ok_or_else(|| format!("missing required argument '{name}'"))
}

fn string_argument(arguments: &BTreeMap<String, Value>, name: &str) -> Result<String, String> {
    argument(arguments, name)?.string().map(str::to_string)
}

fn safe_relative(value: &str) -> Result<PathBuf, String> {
    let path = Path::new(value);
    if value.is_empty()
        || path.is_absolute()
        || path.components().any(|part| {
            matches!(
                part,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(format!("path must stay inside the AML project: {value:?}"));
    }
    Ok(path.to_path_buf())
}

fn source_path(root: &Path, value: &str) -> Result<PathBuf, String> {
    let relative = safe_relative(value)?;
    let joined = root.join(relative);
    if fs::symlink_metadata(&joined).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(format!(
            "symbolic-link source paths are not allowed: {value:?}"
        ));
    }
    let resolved = joined
        .canonicalize()
        .map_err(|error| format!("cannot resolve {value:?}: {error}"))?;
    if !resolved.starts_with(root) || !resolved.is_file() {
        return Err(format!(
            "source is outside the AML project or is not a file: {value:?}"
        ));
    }
    Ok(resolved)
}

fn directory_path(root: &Path, value: &str) -> Result<PathBuf, String> {
    let relative = safe_relative(value)?;
    let joined = root.join(relative);
    if fs::symlink_metadata(&joined).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(format!(
            "symbolic-link test directories are not allowed: {value:?}"
        ));
    }
    let resolved = joined
        .canonicalize()
        .map_err(|error| format!("cannot resolve {value:?}: {error}"))?;
    if !resolved.starts_with(root) || !resolved.is_dir() {
        return Err(format!(
            "test directory is outside the AML project or is not a directory: {value:?}"
        ));
    }
    Ok(resolved)
}

fn output_path(root: &Path, value: &str) -> Result<PathBuf, String> {
    let relative = safe_relative(value)?;
    let joined = root.join(relative);
    let mut candidate = root.to_path_buf();
    for component in safe_relative(value)?.components() {
        candidate.push(component);
        if fs::symlink_metadata(&candidate).is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            return Err(format!(
                "symbolic links are not allowed in output paths: {value:?}"
            ));
        }
    }
    let mut existing = joined.as_path();
    while !existing.exists() {
        existing = existing.parent().ok_or("output has no valid parent")?;
    }
    let resolved_parent = existing.canonicalize().map_err(|error| error.to_string())?;
    if !resolved_parent.starts_with(root) {
        return Err(format!("output escapes the AML project: {value:?}"));
    }
    if fs::symlink_metadata(&joined).is_ok_and(|metadata| metadata.file_type().is_symlink()) {
        return Err(format!("symbolic-link outputs are not allowed: {value:?}"));
    }
    Ok(joined)
}

fn run_aml(arguments: &[String]) -> Result<String, String> {
    let executable = env::current_exe().map_err(|error| error.to_string())?;
    let output = Command::new(executable)
        .args(arguments)
        .output()
        .map_err(|error| format!("cannot run AML compiler: {error}"))?;
    let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
    if output.status.success() {
        Ok(stdout)
    } else if stderr.is_empty() {
        Err(stdout)
    } else {
        Err(stderr)
    }
}

fn call_tool(
    root: &Path,
    name: &str,
    arguments: &BTreeMap<String, Value>,
) -> Result<String, String> {
    match name {
        "aml_project_context" => agent::context(root),
        "aml_doctor" => run_aml(&["doctor".into(), "--json".into()]),
        "aml_targets" => run_aml(&["targets".into(), "--json".into()]),
        "aml_diagnose" | "aml_repair_context" => {
            let path = source_path(root, &string_argument(arguments, "path")?)?;
            let target = arguments
                .get("target")
                .map(Value::string)
                .transpose()?
                .unwrap_or("javascript");
            run_aml(&[
                "diagnose".into(),
                path.to_string_lossy().into_owned(),
                "--target".into(),
                target.into(),
                "--json".into(),
            ])
        }
        "aml_test" => {
            let directory = arguments
                .get("directory")
                .map(Value::string)
                .transpose()?
                .unwrap_or("tests");
            let directory = directory_path(root, directory)?;
            let target = arguments
                .get("target")
                .map(Value::string)
                .transpose()?
                .unwrap_or("javascript");
            run_aml(&[
                "test".into(),
                directory.to_string_lossy().into_owned(),
                "--target".into(),
                target.into(),
                "--json".into(),
            ])
        }
        "aml_check" => {
            let path = source_path(root, &string_argument(arguments, "path")?)?;
            let target = arguments
                .get("target")
                .map(Value::string)
                .transpose()?
                .unwrap_or("javascript");
            run_aml(&[
                "check".into(),
                path.to_string_lossy().into_owned(),
                "--target".into(),
                target.into(),
                "--json".into(),
            ])
        }
        "aml_format" => {
            let path = source_path(root, &string_argument(arguments, "path")?)?;
            let source = fs::read_to_string(&path).map_err(|error| error.to_string())?;
            let formatted = format_source(&source);
            if formatted != source {
                fs::write(&path, formatted).map_err(|error| error.to_string())?;
            }
            Ok(format!(
                "formatted {}",
                path.strip_prefix(root).unwrap().display()
            ))
        }
        "aml_build" => {
            let path = source_path(root, &string_argument(arguments, "path")?)?;
            let target = string_argument(arguments, "target")?;
            let output = output_path(root, &string_argument(arguments, "output")?)?;
            let mut command = vec![
                "build".into(),
                path.to_string_lossy().into_owned(),
                "--target".into(),
                target,
                "-o".into(),
                output.to_string_lossy().into_owned(),
            ];
            if matches!(arguments.get("force"), Some(Value::Bool(true))) {
                command.push("--force".into());
            }
            run_aml(&command)
        }
        _ => Err(format!("unknown AML MCP tool: {name}")),
    }
}

fn request(root: &Path, message: &Value) -> Option<Value> {
    let request = match message.object() {
        Ok(value) => value,
        Err(error_message) => return Some(error(Value::Null, -32600, error_message)),
    };
    let id = request.get("id").cloned();
    if request.get("jsonrpc").and_then(|value| value.string().ok()) != Some("2.0") {
        return id.map(|id| error(id, -32600, "jsonrpc must be exactly '2.0'"));
    }
    let method = match request.get("method").and_then(|value| value.string().ok()) {
        Some(value) => value,
        None => return id.map(|id| error(id, -32600, "request is missing method")),
    };
    let id = id?;
    match method {
        "ping" => Some(result(id, object(Vec::<(String, Value)>::new()))),
        "initialize" => {
            let requested = request.get("params").and_then(|value| value.object().ok()).and_then(|params| params.get("protocolVersion")).and_then(|value| value.string().ok());
            let protocol = requested.filter(|value| *value <= LEGACY_PROTOCOL).unwrap_or(LEGACY_PROTOCOL);
            Some(result(id, object([
                ("protocolVersion", Value::String(protocol.into())),
                ("capabilities", capabilities()),
                ("serverInfo", server_info()),
                ("instructions", Value::String("Use AML tools only for files inside the configured project root. Check after formatting or building.".into())),
            ])))
        }
        "server/discover" => Some(result(id, object([
            ("resultType", Value::String("complete".into())),
            ("supportedVersions", Value::Array(vec![Value::String(MODERN_PROTOCOL.into())])),
            ("capabilities", capabilities()),
            ("instructions", Value::String("Use AML tools only for files inside the configured project root. Check after edits.".into())),
            ("ttlMs", Value::Number("300000".into())),
            ("cacheScope", Value::String("global".into())),
            ("_meta", object([("io.modelcontextprotocol/serverInfo", server_info())])),
        ]))),
        "tools/list" => Some(result(id, object([
            ("resultType", Value::String("complete".into())),
            ("tools", json::parse(TOOL_CATALOG).unwrap()),
            ("ttlMs", Value::Number("300000".into())),
            ("cacheScope", Value::String("global".into())),
        ]))),
        "tools/call" => {
            let params = request.get("params").and_then(|value| value.object().ok());
            let Some(params) = params else { return Some(error(id, -32602, "tools/call requires params")); };
            let Some(name) = params.get("name").and_then(|value| value.string().ok()) else { return Some(error(id, -32602, "tools/call requires a tool name")); };
            let empty = BTreeMap::new();
            let arguments = match params.get("arguments") {
                None => &empty,
                Some(value) => match value.object() { Ok(value) => value, Err(message) => return Some(error(id, -32602, message)) },
            };
            let response = match call_tool(root, name, arguments) {
                Ok(text) => text_result(text, false),
                Err(message) => text_result(message, true),
            };
            Some(result(id, response))
        }
        _ => Some(error(id, -32601, format!("method not found: {method}"))),
    }
}

pub fn serve(directory: &Path) -> Result<(), String> {
    let root = directory.canonicalize().map_err(|error| {
        format!(
            "cannot resolve MCP project root {}: {error}",
            directory.display()
        )
    })?;
    if !root.is_dir() {
        return Err(format!(
            "MCP project root is not a directory: {}",
            root.display()
        ));
    }
    let stdin = io::stdin();
    let mut stdout = io::stdout().lock();
    for line in stdin.lock().lines() {
        let line = line.map_err(|error| error.to_string())?;
        if line.len() > MAX_MESSAGE_BYTES {
            return Err("MCP request exceeds 4 MiB limit".into());
        }
        if line.trim().is_empty() {
            continue;
        }
        let parsed = match json::parse(&line) {
            Ok(value) => value,
            Err(message) => {
                writeln!(
                    stdout,
                    "{}",
                    json::compact(&error(Value::Null, -32700, message))
                )
                .map_err(|error| error.to_string())?;
                stdout.flush().map_err(|error| error.to_string())?;
                continue;
            }
        };
        if let Some(response) = request(&root, &parsed) {
            writeln!(stdout, "{}", json::compact(&response)).map_err(|error| error.to_string())?;
            stdout.flush().map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

pub fn configuration(directory: &Path) -> Result<String, String> {
    let root = directory
        .canonicalize()
        .map_err(|error| format!("cannot resolve {}: {error}", directory.display()))?;
    let executable = env::current_exe().map_err(|error| error.to_string())?;
    let server = object([
        (
            "args",
            Value::Array(vec![
                Value::String("mcp".into()),
                Value::String("--project".into()),
                Value::String(root.to_string_lossy().into_owned()),
            ]),
        ),
        (
            "command",
            Value::String(executable.to_string_lossy().into_owned()),
        ),
    ]);
    Ok(json::pretty(&object([(
        "mcpServers",
        Value::Object(BTreeMap::from([("aml".into(), server)])),
    )])))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn rpc(method: &str, params: Value) -> Value {
        object([
            ("jsonrpc", Value::String("2.0".into())),
            ("id", Value::Number("1".into())),
            ("method", Value::String(method.into())),
            ("params", params),
        ])
    }

    #[test]
    fn lists_tools_and_serves_both_protocol_eras() {
        let root = std::env::temp_dir();
        for method in ["initialize", "server/discover", "tools/list"] {
            let response =
                request(&root, &rpc(method, object(Vec::<(String, Value)>::new()))).unwrap();
            let response = response.object().unwrap();
            assert!(response.contains_key("result"));
        }
        let listed = request(
            &root,
            &rpc("tools/list", object(Vec::<(String, Value)>::new())),
        )
        .unwrap();
        assert!(json::compact(&listed).contains("aml_project_context"));
    }

    #[test]
    fn context_tool_is_rooted_and_traversal_is_rejected() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("aml-mcp-{}-{nonce}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("main.aml"),
            "app safe\nprofile script\n\ntask main:\n  show \"safe\"\n",
        )
        .unwrap();
        let context = call_tool(&root, "aml_project_context", &BTreeMap::new()).unwrap();
        assert!(context.contains("main.aml"));
        let arguments = BTreeMap::from([("path".into(), Value::String("../outside.aml".into()))]);
        assert!(call_tool(&root, "aml_check", &arguments)
            .unwrap_err()
            .contains("inside"));
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn configuration_uses_absolute_paths() {
        let output = configuration(Path::new(".")).unwrap();
        let parsed = json::parse(&output).unwrap();
        let servers = parsed.object().unwrap()["mcpServers"].object().unwrap();
        let aml = servers["aml"].object().unwrap();
        assert!(Path::new(aml["command"].string().unwrap()).is_absolute());
    }
}
