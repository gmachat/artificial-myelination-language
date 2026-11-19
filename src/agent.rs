use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::{
    benchmark,
    json::{self, Value},
    REFERENCE, VERSION,
};

const MAX_FILES: usize = 50;
const MAX_BYTES: usize = 64 * 1024;

fn collect(
    root: &Path,
    directory: &Path,
    files: &mut Vec<(String, String)>,
    bytes: &mut usize,
) -> Result<(), String> {
    let mut entries = fs::read_dir(directory)
        .map_err(|error| format!("cannot read {}: {error}", directory.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| error.to_string())?;
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path).map_err(|error| error.to_string())?;
        if metadata.file_type().is_symlink() {
            continue;
        }
        if metadata.is_dir() {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if name.starts_with('.')
                || matches!(
                    name.as_ref(),
                    "build" | "dist" | "node_modules" | "release" | "target"
                )
            {
                continue;
            }
            collect(root, &path, files, bytes)?;
            continue;
        }
        let name = entry.file_name();
        let include = path.extension().is_some_and(|extension| extension == "aml")
            || matches!(name.to_str(), Some("aml.package.json" | "aml.lock.json"));
        if !include {
            continue;
        }
        if files.len() >= MAX_FILES {
            return Err(format!(
                "project exceeds agent context limit of {MAX_FILES} files"
            ));
        }
        let content = fs::read_to_string(&path)
            .map_err(|error| format!("cannot read {} as UTF-8: {error}", path.display()))?;
        *bytes += content.len();
        if *bytes > MAX_BYTES {
            return Err(format!(
                "project exceeds agent context limit of {MAX_BYTES} bytes"
            ));
        }
        let relative = path
            .strip_prefix(root)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        files.push((relative, content));
    }
    Ok(())
}

pub fn context(directory: &Path) -> Result<String, String> {
    let root: PathBuf = directory
        .canonicalize()
        .map_err(|error| format!("cannot resolve {}: {error}", directory.display()))?;
    if !root.is_dir() {
        return Err(format!("project is not a directory: {}", root.display()));
    }
    let mut files = Vec::new();
    let mut bytes = 0;
    collect(&root, &root, &mut files, &mut bytes)?;
    let estimated_tokens = benchmark::estimate_tokens(REFERENCE)
        + files
            .iter()
            .map(|(_, content)| benchmark::estimate_tokens(content))
            .sum::<usize>();
    let encoded_files = files
        .into_iter()
        .map(|(path, content)| {
            Value::Object(std::collections::BTreeMap::from([
                ("content".into(), Value::String(content)),
                ("path".into(), Value::String(path)),
            ]))
        })
        .collect();
    let value = Value::Object(std::collections::BTreeMap::from([
        (
            "commands".into(),
            Value::Object(std::collections::BTreeMap::from([
                (
                    "build".into(),
                    Value::String("aml build main.aml --target javascript -o build/app.js".into()),
                ),
                (
                    "check".into(),
                    Value::String("aml check main.aml --json".into()),
                ),
                ("format".into(), Value::String("aml format main.aml".into())),
                (
                    "test_run".into(),
                    Value::String("aml run main.aml --target javascript".into()),
                ),
            ])),
        ),
        (
            "constraints".into(),
            Value::Array(vec![
                Value::String("Use only syntax present in language_reference.".into()),
                Value::String("Preserve two-space indentation and run check after edits.".into()),
                Value::String("Return complete file changes; do not edit generated output.".into()),
            ]),
        ),
        (
            "estimated_lexical_tokens".into(),
            Value::Number(estimated_tokens.to_string()),
        ),
        ("files".into(), Value::Array(encoded_files)),
        ("language_reference".into(), Value::String(REFERENCE.into())),
        (
            "root".into(),
            Value::String(root.to_string_lossy().into_owned()),
        ),
        ("schema".into(), Value::String("aml-agent-context-1".into())),
        ("version".into(), Value::String(VERSION.into())),
    ]));
    Ok(json::pretty(&value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn context_is_deterministic_and_excludes_unrelated_files() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("aml-agent-{}-{nonce}", std::process::id()));
        fs::create_dir_all(root.join("build")).unwrap();
        fs::write(
            root.join("main.aml"),
            "app demo\nprofile script\n\ntask main:\n  show \"ok\"\n",
        )
        .unwrap();
        fs::write(root.join(".env"), "SECRET=never").unwrap();
        fs::write(root.join("build/generated.js"), "secret").unwrap();
        let first = context(&root).unwrap();
        let second = context(&root).unwrap();
        assert_eq!(first, second);
        assert!(first.contains("main.aml"));
        assert!(!first.contains("SECRET"));
        assert!(!first.contains("generated.js"));
        json::parse(&first).unwrap();
        let _ = fs::remove_dir_all(root);
    }
}
