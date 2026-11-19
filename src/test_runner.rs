use std::{
    collections::BTreeMap,
    env, fs,
    path::{Path, PathBuf},
    process::Command,
};

use crate::json::{self, Value};

fn discover(directory: &Path, output: &mut Vec<PathBuf>) -> Result<(), String> {
    let mut entries = fs::read_dir(directory)
        .map_err(|error| format!("cannot read {}: {error}", directory.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        if entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_symlink()
        {
            continue;
        }
        if path.is_dir() {
            discover(&path, output)?;
        } else if path.extension().is_some_and(|extension| extension == "aml") {
            output.push(path);
        }
    }
    Ok(())
}

pub struct TestReport {
    pub json: String,
    pub failed: usize,
}

pub fn run(directory: &Path, target: &str) -> Result<TestReport, String> {
    let root = directory
        .canonicalize()
        .map_err(|error| format!("cannot resolve {}: {error}", directory.display()))?;
    let mut paths = Vec::new();
    discover(&root, &mut paths)?;
    if paths.is_empty() {
        return Err(format!("no .aml tests found in {}", root.display()));
    }
    let executable = env::current_exe().map_err(|error| error.to_string())?;
    let mut cases = Vec::new();
    let mut failed = 0;
    for path in paths {
        let output = Command::new(&executable)
            .args(["run", &path.to_string_lossy(), "--target", target])
            .output()
            .map_err(|error| format!("cannot run {}: {error}", path.display()))?;
        let passed = output.status.success();
        if !passed {
            failed += 1;
        }
        cases.push(Value::Object(BTreeMap::from([
            (
                "error".into(),
                Value::String(String::from_utf8_lossy(&output.stderr).trim().to_string()),
            ),
            (
                "name".into(),
                Value::String(
                    path.strip_prefix(&root)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                ),
            ),
            (
                "output".into(),
                Value::String(String::from_utf8_lossy(&output.stdout).trim().to_string()),
            ),
            ("passed".into(), Value::Bool(passed)),
        ])));
    }
    let passed = cases.len() - failed;
    let value = Value::Object(BTreeMap::from([
        ("cases".into(), Value::Array(cases)),
        ("failed".into(), Value::Number(failed.to_string())),
        ("passed".into(), Value::Number(passed.to_string())),
        ("schema".into(), Value::String("aml-test-report-1".into())),
        ("target".into(), Value::String(target.into())),
    ]));
    Ok(TestReport {
        json: json::pretty(&value),
        failed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn discovery_is_sorted_and_skips_non_aml_files() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("aml-tests-{}-{nonce}", std::process::id()));
        fs::create_dir_all(root.join("nested")).unwrap();
        fs::write(root.join("b.aml"), "").unwrap();
        fs::write(root.join("nested/a.aml"), "").unwrap();
        fs::write(root.join("ignore.txt"), "").unwrap();
        let mut paths = Vec::new();
        discover(&root, &mut paths).unwrap();
        assert_eq!(paths.len(), 2);
        assert!(paths.iter().all(|path| path.extension().unwrap() == "aml"));
        let _ = fs::remove_dir_all(root);
    }
}
