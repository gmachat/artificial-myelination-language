use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use crate::{
    json::{self, Value},
    sha256,
};

pub const MANIFEST: &str = "aml.package.json";
pub const LOCKFILE: &str = "aml.lock.json";

fn normalized(value: &str) -> String {
    let mut output = String::new();
    for character in value.to_lowercase().chars() {
        if character.is_ascii_alphanumeric() || character == '_' {
            output.push(character);
        } else if !output.ends_with('_') {
            output.push('_');
        }
    }
    let value = output.trim_matches('_');
    if value.is_empty() {
        "aml_project".into()
    } else {
        value.into()
    }
}

fn read_json(path: &Path) -> Result<Value, String> {
    let source = fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    json::parse(&source).map_err(|error| format!("invalid {}: {error}", path.display()))
}

fn field<'a>(object: &'a BTreeMap<String, Value>, name: &str) -> Result<&'a Value, String> {
    object
        .get(name)
        .ok_or_else(|| format!("manifest is missing '{name}'"))
}

pub fn init(directory: &Path, name: Option<&str>) -> Result<Vec<PathBuf>, String> {
    fs::create_dir_all(directory)
        .map_err(|error| format!("cannot create {}: {error}", directory.display()))?;
    let manifest = directory.join(MANIFEST);
    let entry = directory.join("main.aml");
    if manifest.exists() {
        return Err(format!("manifest already exists: {}", manifest.display()));
    }
    let fallback = directory
        .file_name()
        .and_then(|value| value.to_str())
        .unwrap_or("aml_project");
    let package_name = normalized(name.unwrap_or(fallback));
    let value = Value::Object(BTreeMap::from([
        ("dependencies".into(), Value::Object(BTreeMap::new())),
        ("entry".into(), Value::String("main.aml".into())),
        ("name".into(), Value::String(package_name.clone())),
        ("version".into(), Value::String("0.1.0".into())),
    ]));
    fs::write(&manifest, json::pretty(&value))
        .map_err(|error| format!("cannot write {}: {error}", manifest.display()))?;
    let mut created = vec![manifest];
    if !entry.exists() {
        fs::write(
            &entry,
            format!(
                "app {package_name}\nprofile script\n\ntask main:\n  show \"Hello from AML\"\n"
            ),
        )
        .map_err(|error| format!("cannot write {}: {error}", entry.display()))?;
        created.push(entry);
    }
    Ok(created)
}

pub fn add(directory: &Path, name: &str, dependency: &Path) -> Result<PathBuf, String> {
    let manifest_path = directory.join(MANIFEST);
    let mut manifest = read_json(&manifest_path)?;
    let dependency = dependency
        .canonicalize()
        .map_err(|error| format!("cannot resolve {}: {error}", dependency.display()))?;
    let dependency_manifest = dependency.join(MANIFEST);
    if !dependency_manifest.is_file() {
        return Err(format!(
            "dependency has no {MANIFEST}: {}",
            dependency.display()
        ));
    }
    read_json(&dependency_manifest)?;
    let directory = directory
        .canonicalize()
        .map_err(|error| format!("cannot resolve {}: {error}", directory.display()))?;
    let stored = dependency
        .strip_prefix(&directory)
        .map(Path::to_path_buf)
        .unwrap_or_else(|_| dependency.clone());
    let Value::Object(root) = &mut manifest else {
        return Err("package manifest must be a JSON object".into());
    };
    let dependencies = root
        .entry("dependencies".into())
        .or_insert_with(|| Value::Object(BTreeMap::new()));
    let Value::Object(dependencies) = dependencies else {
        return Err("manifest dependencies must be an object".into());
    };
    dependencies.insert(
        normalized(name),
        Value::Object(BTreeMap::from([(
            "path".into(),
            Value::String(stored.to_string_lossy().into_owned()),
        )])),
    );
    fs::write(&manifest_path, json::pretty(&manifest))
        .map_err(|error| format!("cannot write {}: {error}", manifest_path.display()))?;
    Ok(manifest_path)
}

fn aml_files(directory: &Path, output: &mut Vec<PathBuf>) -> Result<(), String> {
    for entry in fs::read_dir(directory)
        .map_err(|error| format!("cannot read {}: {error}", directory.display()))?
    {
        let path = entry.map_err(|error| error.to_string())?.path();
        if path.is_dir() {
            aml_files(&path, output)?;
        } else if path.extension().is_some_and(|value| value == "aml") {
            output.push(path);
        }
    }
    Ok(())
}

fn digest(directory: &Path) -> Result<String, String> {
    let mut paths = Vec::new();
    aml_files(directory, &mut paths)?;
    paths.sort();
    let mut input = Vec::new();
    for path in paths {
        let relative = path
            .strip_prefix(directory)
            .unwrap()
            .to_string_lossy()
            .replace('\\', "/");
        input.extend(relative.as_bytes());
        input.push(0);
        input.extend(
            fs::read(&path).map_err(|error| format!("cannot read {}: {error}", path.display()))?,
        );
        input.push(0);
    }
    input.extend(
        fs::read(directory.join(MANIFEST))
            .map_err(|error| format!("cannot read manifest: {error}"))?,
    );
    Ok(sha256::hex(&input))
}

pub fn lock(directory: &Path) -> Result<PathBuf, String> {
    let directory = directory
        .canonicalize()
        .map_err(|error| format!("cannot resolve {}: {error}", directory.display()))?;
    let manifest = read_json(&directory.join(MANIFEST))?;
    let root = manifest.object()?;
    let package = field(root, "name")?.string()?.to_string();
    let dependencies = field(root, "dependencies")?.object()?;
    let mut locked = BTreeMap::new();
    for (name, declaration) in dependencies {
        let raw = PathBuf::from(field(declaration.object()?, "path")?.string()?);
        let dependency = if raw.is_absolute() {
            raw
        } else {
            directory.join(raw)
        };
        let dependency = dependency
            .canonicalize()
            .map_err(|error| format!("cannot resolve dependency '{name}': {error}"))?;
        let dependency_manifest = read_json(&dependency.join(MANIFEST))?;
        let dependency_root = dependency_manifest.object()?;
        locked.insert(
            name.clone(),
            Value::Object(BTreeMap::from([
                (
                    "name".into(),
                    Value::String(field(dependency_root, "name")?.string()?.into()),
                ),
                (
                    "path".into(),
                    Value::String(dependency.to_string_lossy().into_owned()),
                ),
                ("sha256".into(), Value::String(digest(&dependency)?)),
                (
                    "version".into(),
                    Value::String(field(dependency_root, "version")?.string()?.into()),
                ),
            ])),
        );
    }
    let value = Value::Object(BTreeMap::from([
        ("dependencies".into(), Value::Object(locked)),
        ("lock_version".into(), Value::Number("1".into())),
        ("package".into(), Value::String(package)),
    ]));
    let target = directory.join(LOCKFILE);
    fs::write(&target, json::pretty(&value))
        .map_err(|error| format!("cannot write {}: {error}", target.display()))?;
    Ok(target)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};
    #[test]
    fn creates_and_locks_packages_deterministically() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("aml-packages-{}-{nonce}", std::process::id()));
        let dependency = root.join("dependency");
        let application = root.join("application");
        init(&dependency, Some("utility")).unwrap();
        init(&application, Some("app")).unwrap();
        add(&application, "utility", &dependency).unwrap();
        let first = fs::read_to_string(lock(&application).unwrap()).unwrap();
        let second = fs::read_to_string(lock(&application).unwrap()).unwrap();
        let parsed = json::parse(&first).unwrap();
        let utility = &parsed.object().unwrap()["dependencies"].object().unwrap()["utility"];
        assert_eq!(first, second);
        assert_eq!(
            utility.object().unwrap()["sha256"].string().unwrap().len(),
            64
        );
        let _ = fs::remove_dir_all(root);
    }
}
