use std::{collections::BTreeMap, fs, path::Path};

use crate::{
    benchmark, c_backend, csharp, fullstack, go, ir, java, javascript,
    json::{self, Value},
    modules, python, react, ruby, rust_backend, swift, web, CompileError,
};

fn validate(path: &Path, target: &str) -> Result<(), CompileError> {
    let modular = matches!(
        target,
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
    );
    let source = if modular {
        modules::load_program(path)?
    } else {
        fs::read_to_string(path)
            .map_err(|error| CompileError::new(format!("cannot read source: {error}"), 1))?
    };
    match target {
        "javascript" | "js" => javascript::compile_source(&source).map(|_| ()),
        "python" => python::compile_source(&source).map(|_| ()),
        "ruby" => ruby::compile_source(&source).map(|_| ()),
        "go" => go::compile_source(&source).map(|_| ()),
        "rust" => rust_backend::compile_source(&source).map(|_| ()),
        "c" => c_backend::compile_source(&source).map(|_| ()),
        "java" => java::compile_source(&source).map(|_| ()),
        "csharp" | "cs" => csharp::compile_source(&source).map(|_| ()),
        "swift" => swift::compile_source(&source).map(|_| ()),
        "ir" => ir::compile_source(&source, &path.display().to_string()).map(|_| ()),
        "react" => react::compile(&source).map(|_| ()),
        "web" => web::project_files(&source).map(|_| ()),
        "fullstack" => fullstack::project_files(&source).map(|_| ()),
        _ => Err(CompileError::new(format!("unknown target '{target}'"), 1)),
    }
}

fn category(message: &str) -> (&'static str, &'static str) {
    let lower = message.to_lowercase();
    if lower.contains("unknown")
        || lower.contains("not defined")
        || lower.contains("known aml task")
        || lower.contains("declared record")
    {
        (
            "unknown_symbol",
            "Correct the name or add the missing declaration before using it.",
        )
    } else if lower.contains("type")
        || lower.contains("expects")
        || lower.contains("expected") && lower.contains("got")
    {
        (
            "type_mismatch",
            "Adjust the declared type, argument, or expression so expected and actual types agree.",
        )
    } else if lower.contains("indent") || lower.contains("expected") || lower.contains("must use") {
        (
            "syntax",
            "Use the canonical AML form shown in the compact language reference.",
        )
    } else if lower.contains("require") || lower.contains("ensure") || lower.contains("contract") {
        (
            "contract",
            "Make the input or result satisfy the declared contract.",
        )
    } else {
        (
            "compiler",
            "Fix the reported source construct and run the verification command again.",
        )
    }
}

fn expected_actual(message: &str) -> (Value, Value) {
    for marker in [" expects ", " expected "] {
        if let Some((_, rest)) = message.split_once(marker) {
            if let Some((expected, actual)) = rest.split_once(", got ") {
                return (
                    Value::String(expected.trim().to_string()),
                    Value::String(actual.trim().to_string()),
                );
            }
        }
    }
    (Value::Null, Value::Null)
}

fn source_window(source: &str, line: usize) -> Value {
    let lines = source.lines().collect::<Vec<_>>();
    let start = line.saturating_sub(3);
    let end = (line + 2).min(lines.len());
    let text = lines[start..end]
        .iter()
        .enumerate()
        .map(|(offset, value)| format!("{:>4} | {}", start + offset + 1, value))
        .collect::<Vec<_>>()
        .join("\n");
    Value::Object(BTreeMap::from([
        ("end_line".into(), Value::Number(end.to_string())),
        ("start_line".into(), Value::Number((start + 1).to_string())),
        ("text".into(), Value::String(text)),
    ]))
}

pub fn report(path: &Path, target: &str) -> Result<String, String> {
    let source = fs::read_to_string(path)
        .map_err(|error| format!("cannot read {}: {error}", path.display()))?;
    let verification = format!("aml check {} --target {} --json", path.display(), target);
    let diagnostics = match validate(path, target) {
        Ok(()) => Vec::new(),
        Err(error) => {
            let (category, suggestion) = category(&error.message);
            let (expected, actual) = expected_actual(&error.message);
            vec![Value::Object(BTreeMap::from([
                ("actual".into(), actual),
                ("category".into(), Value::String(category.into())),
                (
                    "code".into(),
                    Value::String(
                        match category {
                            "syntax" => "AML1100",
                            "unknown_symbol" => "AML1200",
                            "type_mismatch" => "AML1300",
                            "contract" => "AML1400",
                            _ => "AML1000",
                        }
                        .into(),
                    ),
                ),
                ("column".into(), Value::Number(error.column.to_string())),
                ("expected".into(), expected),
                ("line".into(), Value::Number(error.line.to_string())),
                ("message".into(), Value::String(error.message)),
                ("source_window".into(), source_window(&source, error.line)),
                ("suggestion".into(), Value::String(suggestion.into())),
            ]))]
        }
    };
    let valid = diagnostics.is_empty();
    let value = Value::Object(BTreeMap::from([
        ("diagnostics".into(), Value::Array(diagnostics)),
        (
            "estimated_lexical_tokens".into(),
            Value::Number(benchmark::estimate_tokens(&source).to_string()),
        ),
        ("path".into(), Value::String(path.display().to_string())),
        (
            "schema".into(),
            Value::String("aml-repair-context-1".into()),
        ),
        ("target".into(), Value::String(target.into())),
        ("valid".into(), Value::Bool(valid)),
        ("verification".into(), Value::String(verification)),
    ]));
    Ok(json::pretty(&value))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn report_contains_small_actionable_window() {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("aml-diagnose-{}-{nonce}.aml", std::process::id()));
        fs::write(
            &path,
            "app broken\nprofile script\n\ntask main:\n  show missing(1)\n",
        )
        .unwrap();
        let output = report(&path, "javascript").unwrap();
        let parsed = json::parse(&output).unwrap();
        assert_eq!(parsed.object().unwrap()["valid"], Value::Bool(false));
        assert!(output.contains("unknown_symbol"));
        assert!(output.contains("source_window"));
        assert!(output.len() < 2_500);
        let _ = fs::remove_file(path);
    }
}
