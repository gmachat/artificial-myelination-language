use std::{
    collections::{BTreeMap, HashSet},
    fs,
    path::{Path, PathBuf},
};

use crate::CompileError;

fn import_path(text: &str) -> Option<&str> {
    text.strip_prefix("use \"")?.strip_suffix('"')
}

fn declaration(text: &str) -> Option<&str> {
    let raw = text
        .strip_prefix("task ")
        .or_else(|| text.strip_prefix("record "))?;
    let end = raw
        .find(|character: char| !(character.is_ascii_alphanumeric() || character == '_'))
        .unwrap_or(raw.len());
    (end > 0).then_some(&raw[..end])
}

struct Loader {
    visited: HashSet<PathBuf>,
    active: Vec<PathBuf>,
    modules: Vec<String>,
    declarations: BTreeMap<String, PathBuf>,
}

impl Loader {
    fn visit(&mut self, requested: &Path, entry: bool) -> Result<String, CompileError> {
        let path = requested.canonicalize().map_err(|_| {
            CompileError::new(format!("module not found: {}", requested.display()), 1)
        })?;
        if let Some(index) = self.active.iter().position(|item| item == &path) {
            let cycle = self.active[index..]
                .iter()
                .chain(std::iter::once(&path))
                .filter_map(|item| item.file_name())
                .map(|item| item.to_string_lossy())
                .collect::<Vec<_>>()
                .join(" -> ");
            return Err(CompileError::new(format!("module cycle: {cycle}"), 1));
        }
        if self.visited.contains(&path) {
            return Ok(String::new());
        }
        let source = fs::read_to_string(&path).map_err(|error| {
            CompileError::new(format!("cannot read module {}: {error}", path.display()), 1)
        })?;
        self.active.push(path.clone());
        let mut body = Vec::new();
        let mut header_seen = false;
        for (offset, raw) in source.lines().enumerate() {
            let line = offset + 1;
            let text = raw.trim();
            if text.is_empty() || text.starts_with('#') {
                body.push(raw.to_string());
                continue;
            }
            if !header_seen {
                let prefix = if entry { "app " } else { "module " };
                if !text.starts_with(prefix) {
                    return Err(CompileError::new(
                        format!(
                            "{} file must begin with '{} <name>'",
                            if entry { "entry" } else { "module" },
                            prefix.trim()
                        ),
                        line,
                    ));
                }
                header_seen = true;
                if entry {
                    body.push(raw.to_string());
                }
                continue;
            }
            if let Some(relative) = import_path(text) {
                if raw.starts_with(char::is_whitespace) {
                    return Err(CompileError::new(
                        "use declarations must be at application level",
                        line,
                    ));
                }
                let imported = path.parent().unwrap_or(Path::new(".")).join(relative);
                let imported_body = self.visit(&imported, false)?;
                if !imported_body.is_empty() {
                    self.modules.push(imported_body);
                }
                continue;
            }
            if !raw.starts_with(char::is_whitespace) {
                if let Some(name) = declaration(text) {
                    if let Some(previous) = self.declarations.get(name) {
                        return Err(CompileError::new(
                            format!(
                                "duplicate declaration '{}' in {} and {}",
                                name,
                                previous.file_name().unwrap_or_default().to_string_lossy(),
                                path.file_name().unwrap_or_default().to_string_lossy()
                            ),
                            line,
                        ));
                    }
                    self.declarations.insert(name.into(), path.clone());
                }
            }
            body.push(raw.to_string());
        }
        self.active.pop();
        self.visited.insert(path);
        if !header_seen {
            return Err(CompileError::new("empty AML module", 1));
        }
        Ok(body.join("\n").trim().to_string() + "\n")
    }
}

pub fn load_program(path: &Path) -> Result<String, CompileError> {
    let mut loader = Loader {
        visited: HashSet::new(),
        active: Vec::new(),
        modules: Vec::new(),
        declarations: BTreeMap::new(),
    };
    let entry = loader.visit(path, true)?;
    if loader.modules.is_empty() {
        return Ok(entry);
    }
    let lines: Vec<_> = entry.lines().collect();
    let mut insert = 1;
    while insert < lines.len()
        && (lines[insert].trim().is_empty()
            || lines[insert].trim_start().starts_with('#')
            || lines[insert].trim().starts_with("profile "))
    {
        insert += 1;
    }
    let mut output = lines[..insert]
        .iter()
        .map(|line| (*line).to_string())
        .collect::<Vec<_>>();
    output.push(String::new());
    for module in loader.modules {
        output.extend(module.trim().lines().map(str::to_string));
        output.push(String::new());
    }
    output.extend(lines[insert..].iter().map(|line| (*line).to_string()));
    Ok(output.join("\n").trim_end().to_string() + "\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn root() -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("aml-modules-{}-{nonce}", std::process::id()));
        fs::create_dir_all(&path).unwrap();
        path
    }

    #[test]
    fn loads_local_modules() {
        let root = root();
        fs::write(
            root.join("math.aml"),
            "module math\ntask double(value: Int) -> Int:\n  return value * 2\n",
        )
        .unwrap();
        fs::write(
            root.join("main.aml"),
            "app modules\nprofile script\nuse \"math.aml\"\ntask main:\n  show double(21)\n",
        )
        .unwrap();
        let source = load_program(&root.join("main.aml")).unwrap();
        let _ = fs::remove_dir_all(root);
        assert!(!source.contains("use \""));
        assert!(source.contains("task double"));
        assert!(source.contains("task main"));
    }

    #[test]
    fn rejects_cycles() {
        let root = root();
        fs::write(
            root.join("a.aml"),
            "module a\nuse \"b.aml\"\ntask a:\n  show 1\n",
        )
        .unwrap();
        fs::write(
            root.join("b.aml"),
            "module b\nuse \"a.aml\"\ntask b:\n  show 2\n",
        )
        .unwrap();
        fs::write(
            root.join("main.aml"),
            "app cycle\nprofile script\nuse \"a.aml\"\ntask main:\n  show 3\n",
        )
        .unwrap();
        let error = load_program(&root.join("main.aml")).unwrap_err();
        let _ = fs::remove_dir_all(root);
        assert!(error.message.contains("module cycle"));
    }
}
