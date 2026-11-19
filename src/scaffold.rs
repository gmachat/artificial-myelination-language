use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::packages;

fn project_name(value: &str) -> Result<String, String> {
    let mut output = String::new();
    for character in value.to_lowercase().chars() {
        if character.is_ascii_alphanumeric() || character == '_' {
            output.push(character);
        } else if character == '-' || character == ' ' {
            output.push('_');
        } else {
            return Err(format!(
                "project name contains unsupported character '{character}'"
            ));
        }
    }
    let output = output.trim_matches('_').to_string();
    if output.is_empty() || output.starts_with(|character: char| character.is_ascii_digit()) {
        Err("project name must start with a letter or underscore".into())
    } else {
        Ok(output)
    }
}

fn source(name: &str, profile: &str) -> Result<String, String> {
    match profile {
        "script" => Ok(format!("app {name}\nprofile script\n\ntask main:\n  show \"Hello from AML\"\n")),
        "react" => Ok(format!("app {name}\nprofile react\n\ncomponent App(title: Text):\n  state count: Int = 0\n\n  view:\n    column:\n      heading title\n      text \"Count: {{count}}\"\n      button \"Add one\":\n        on click:\n          set count = count + 1\n")),
        "web" => Ok(format!("app {name}\nprofile web\n\ncomponent App(title: Text):\n  state count: Int = 0\n\n  view:\n    column:\n      heading title\n      text \"Count: {{count}}\"\n      button \"Add one\":\n        on click:\n          set count = count + 1\n")),
        "fullstack" => Ok(format!("app {name}\nprofile fullstack\n\ndata Product:\n  name: Text\n  quantity: Int\n  active: Bool\n\nresource products: Product:\n  list\n  create\n  update\n  delete\n\npage Dashboard at \"/\":\n  resource products\n")),
        _ => Err(format!("unknown profile '{profile}'; expected script, react, web, or fullstack")),
    }
}

pub fn create(
    directory: &Path,
    requested_name: &str,
    profile: &str,
) -> Result<Vec<PathBuf>, String> {
    if directory.is_dir()
        && fs::read_dir(directory)
            .map_err(|error| error.to_string())?
            .next()
            .is_some()
    {
        return Err(format!(
            "project directory is not empty: {}",
            directory.display()
        ));
    }
    let name = project_name(requested_name)?;
    let mut created = packages::init(directory, Some(&name))?;
    let entry = directory.join("main.aml");
    fs::write(&entry, source(&name, profile)?)
        .map_err(|error| format!("cannot write {}: {error}", entry.display()))?;
    if !created.contains(&entry) {
        created.push(entry);
    }
    Ok(created)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn creates_each_supported_profile() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("aml-scaffold-{}-{nonce}", std::process::id()));
        for profile in ["script", "react", "web", "fullstack"] {
            let directory = root.join(profile);
            create(&directory, &format!("My-{profile}"), profile).unwrap();
            let generated = fs::read_to_string(directory.join("main.aml")).unwrap();
            assert!(generated.contains(&format!("profile {profile}")));
            assert!(directory.join(packages::MANIFEST).is_file());
        }
        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn rejects_nonempty_directories() {
        let root =
            std::env::temp_dir().join(format!("aml-scaffold-existing-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("keep.txt"), "keep").unwrap();
        assert!(create(&root, "app", "script")
            .unwrap_err()
            .contains("not empty"));
        assert_eq!(fs::read_to_string(root.join("keep.txt")).unwrap(), "keep");
        let _ = fs::remove_dir_all(root);
    }
}
