use std::{
    collections::{BTreeMap, HashSet},
    fs,
    io::{Read, Write},
    path::{Component, Path},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

use crate::json::{self, Value};

pub const PROTOCOL: &str = "aml-backend-1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Artifact {
    pub path: String,
    pub content: String,
    pub executable: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackendResult {
    pub artifacts: Vec<Artifact>,
    pub entry: Option<String>,
    pub diagnostics: Vec<BTreeMap<String, Value>>,
}

fn object(value: &Value) -> Result<&BTreeMap<String, Value>, String> {
    value.object()
}
fn required<'a>(value: &'a BTreeMap<String, Value>, key: &str) -> Result<&'a Value, String> {
    value
        .get(key)
        .ok_or_else(|| format!("backend response is missing '{key}'"))
}

fn safe_path(value: &Value) -> Result<String, String> {
    let value = value.string()?;
    if value.is_empty() || value.starts_with(['/', '\\']) || value.contains('\\') {
        return Err(format!("unsafe artifact path: {value:?}"));
    }
    let path = Path::new(value);
    if path.is_absolute()
        || path.components().any(|part| {
            matches!(
                part,
                Component::ParentDir | Component::RootDir | Component::Prefix(_)
            )
        })
    {
        return Err(format!("unsafe artifact path: {value:?}"));
    }
    Ok(value.into())
}

pub fn parse_response(source: &str) -> Result<BackendResult, String> {
    let parsed =
        json::parse(source).map_err(|error| format!("backend returned invalid JSON: {error}"))?;
    let root = object(&parsed)?;
    if required(root, "protocol")?.string()? != PROTOCOL {
        return Err(format!("backend response must use protocol {PROTOCOL:?}"));
    }
    let Value::Array(raw_artifacts) = required(root, "artifacts")? else {
        return Err("backend response needs artifacts".into());
    };
    if raw_artifacts.is_empty() {
        return Err("backend response needs at least one artifact".into());
    }
    let mut artifacts = Vec::new();
    let mut seen = HashSet::new();
    for raw in raw_artifacts {
        let raw = object(raw)?;
        let path = safe_path(required(raw, "path")?)?;
        if !seen.insert(path.clone()) {
            return Err(format!("duplicate artifact path: {path:?}"));
        }
        let content = required(raw, "content")?.string()?.to_string();
        let executable = match raw.get("executable") {
            None => false,
            Some(Value::Bool(value)) => *value,
            Some(_) => return Err(format!("artifact {path:?} executable must be boolean")),
        };
        artifacts.push(Artifact {
            path,
            content,
            executable,
        });
    }
    let entry = match root.get("entry") {
        None | Some(Value::Null) => None,
        Some(value) => Some(safe_path(value)?),
    };
    if entry.as_ref().is_some_and(|value| !seen.contains(value)) {
        return Err(format!("entry {:?} is not an artifact", entry.unwrap()));
    }
    let diagnostics = match root.get("diagnostics") {
        None => Vec::new(),
        Some(Value::Array(values)) => values
            .iter()
            .map(|value| value.object().cloned())
            .collect::<Result<Vec<_>, _>>()?,
        Some(_) => return Err("diagnostics must be a list of objects".into()),
    };
    Ok(BackendResult {
        artifacts,
        entry,
        diagnostics,
    })
}

fn request(ir: Value, source: &str) -> String {
    json::pretty(&Value::Object(BTreeMap::from([
        ("ir".into(), ir),
        ("protocol".into(), Value::String(PROTOCOL.into())),
        ("source".into(), Value::String(source.into())),
    ])))
}

pub fn run(
    executable: &Path,
    ir: Value,
    source: &str,
    timeout: Duration,
) -> Result<BackendResult, String> {
    let mut child = Command::new(executable)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|error| format!("cannot run backend {}: {error}", executable.display()))?;
    child
        .stdin
        .take()
        .unwrap()
        .write_all(request(ir, source).as_bytes())
        .map_err(|error| format!("cannot write backend request: {error}"))?;
    let mut stdout_pipe = child.stdout.take().unwrap();
    let mut stderr_pipe = child.stderr.take().unwrap();
    let stdout_thread = thread::spawn(move || {
        let mut value = String::new();
        let result = stdout_pipe.read_to_string(&mut value);
        (result, value)
    });
    let stderr_thread = thread::spawn(move || {
        let mut value = String::new();
        let result = stderr_pipe.read_to_string(&mut value);
        (result, value)
    });
    let start = Instant::now();
    let status = loop {
        if let Some(status) = child
            .try_wait()
            .map_err(|error| format!("cannot wait for backend: {error}"))?
        {
            break status;
        }
        if start.elapsed() >= timeout {
            let _ = child.kill();
            let _ = child.wait();
            return Err(format!(
                "backend timed out after {} seconds",
                timeout.as_secs()
            ));
        }
        thread::sleep(Duration::from_millis(10));
    };
    let (stdout_result, stdout) = stdout_thread
        .join()
        .map_err(|_| "backend stdout reader failed")?;
    let (stderr_result, stderr) = stderr_thread
        .join()
        .map_err(|_| "backend stderr reader failed")?;
    stdout_result.map_err(|error| error.to_string())?;
    stderr_result.map_err(|error| error.to_string())?;
    if !status.success() {
        return Err(format!(
            "backend failed: {}",
            if stderr.trim().is_empty() {
                status.to_string()
            } else {
                stderr.trim().into()
            }
        ));
    }
    parse_response(&stdout)
}

pub fn write_result(result: &BackendResult, directory: &Path) -> Result<(), String> {
    fs::create_dir_all(directory)
        .map_err(|error| format!("cannot create {}: {error}", directory.display()))?;
    let root = directory
        .canonicalize()
        .map_err(|error| error.to_string())?;
    for artifact in &result.artifacts {
        let destination = directory.join(&artifact.path);
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        }
        let resolved_parent = destination
            .parent()
            .unwrap_or(directory)
            .canonicalize()
            .map_err(|error| error.to_string())?;
        if resolved_parent != root && !resolved_parent.starts_with(&root) {
            return Err(format!(
                "artifact escapes output directory: {:?}",
                artifact.path
            ));
        }
        if fs::symlink_metadata(&destination)
            .is_ok_and(|metadata| metadata.file_type().is_symlink())
        {
            return Err(format!(
                "artifact destination is a symbolic link: {:?}",
                artifact.path
            ));
        }
        fs::write(&destination, &artifact.content)
            .map_err(|error| format!("cannot write {}: {error}", destination.display()))?;
        #[cfg(unix)]
        if artifact.executable {
            use std::os::unix::fs::PermissionsExt;
            let mut permissions = fs::metadata(&destination)
                .map_err(|error| error.to_string())?
                .permissions();
            permissions.set_mode(permissions.mode() | 0o111);
            fs::set_permissions(&destination, permissions).map_err(|error| error.to_string())?;
        }
    }
    Ok(())
}

const FIXTURES: [(&str, &str); 3] = [
    ("scalar", "app scalar\nprofile script\ntask double(value: Int) -> Int:\n  return value * 2\ntask main:\n  show double(21)\n"),
    ("structured", "app structured\nprofile script\nrecord Item:\n  name: Text\n  values: List[Int]\n  note: Optional[Text]\ntask first(item: Item) -> Int:\n  return item.values[0]\ntask main:\n  show first({name: \"sample\", values: [42], note: none})\n"),
    ("contracts", "app contracts\nprofile script\ntask classify(value: Int) -> Text:\n  require value >= 0\n  ensure result != \"\"\n  if value > 0:\n    return \"positive\"\n  else:\n    return \"zero\"\ntask main:\n  show classify(1)\n"),
];

pub fn check(executable: &Path) -> Result<Value, String> {
    let mut cases = Vec::new();
    for (name, source) in FIXTURES {
        let ir_source = crate::ir::compile_source(source, &format!("{name}.aml"))
            .map_err(|error| error.to_string())?;
        let ir = json::parse(&ir_source)?;
        let first = run(
            executable,
            ir.clone(),
            &format!("{name}.aml"),
            Duration::from_secs(30),
        )?;
        let second = run(
            executable,
            ir,
            &format!("{name}.aml"),
            Duration::from_secs(30),
        )?;
        if first != second {
            return Err(format!(
                "backend output is not deterministic for fixture {name:?}"
            ));
        }
        cases.push(Value::Object(BTreeMap::from([
            (
                "artifacts".into(),
                Value::Number(first.artifacts.len().to_string()),
            ),
            (
                "entry".into(),
                first.entry.map(Value::String).unwrap_or(Value::Null),
            ),
            ("name".into(), Value::String(name.into())),
            ("valid".into(), Value::Bool(true)),
        ])));
    }
    Ok(Value::Object(BTreeMap::from([
        (
            "backend".into(),
            Value::String(executable.display().to_string()),
        ),
        ("cases".into(), Value::Array(cases)),
        ("protocol".into(), Value::String(PROTOCOL.into())),
        ("valid".into(), Value::Bool(true)),
    ])))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rejects_escape() {
        let source =
            r#"{"protocol":"aml-backend-1","artifacts":[{"path":"../bad","content":"bad"}]}"#;
        assert!(parse_response(source)
            .unwrap_err()
            .contains("unsafe artifact path"));
    }
    #[test]
    fn parses_artifact() {
        let source = r#"{"protocol":"aml-backend-1","entry":"main.txt","artifacts":[{"path":"main.txt","content":"ok\n"}]}"#;
        let result = parse_response(source).unwrap();
        assert_eq!(result.entry.as_deref(), Some("main.txt"));
    }
}
