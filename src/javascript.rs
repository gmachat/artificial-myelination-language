use crate::{
    core::{Expression, Program, Statement, Task},
    expression::render_javascript_expr,
};

const RUNTIME: &str = r#"class AMLFailure extends Error {}

function __amlActual(value) {
  if (value === null) return "none";
  if (Array.isArray(value)) return "list";
  return typeof value;
}

function __amlExpect(place, value, expected) {
  if (!__amlValid(value, expected)) {
    throw new AMLFailure(`type | ${place} expected ${expected}, got ${__amlActual(value)}`);
  }
}

function __amlValid(value, expected) {
  if (expected === "Any") return true;
  if (expected.startsWith("Optional[")) return value === null || __amlValid(value, expected.slice(9, -1));
  if (expected.startsWith("List[")) return Array.isArray(value) && value.every(item => __amlValid(item, expected.slice(5, -1)));
  if (Object.hasOwn(__amlRecords, expected)) {
    const fields = __amlRecords[expected];
    return value !== null && !Array.isArray(value) && typeof value === "object" &&
      Object.keys(value).length === Object.keys(fields).length &&
      Object.entries(fields).every(([name, type]) => Object.hasOwn(value, name) && __amlValid(value[name], type));
  }
  return (expected === "Bool" && typeof value === "boolean") ||
    (expected === "Float" && typeof value === "number" && Number.isFinite(value)) ||
    (expected === "Int" && Number.isInteger(value)) ||
    (expected === "Text" && typeof value === "string");
}

function __amlRequire(condition, task, kind, expression) {
  if (!condition) throw new AMLFailure(`contract | ${task} ${kind} failed: ${expression}`);
}"#;

fn quote(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
    )
}

fn render(value: &Expression) -> String {
    render_javascript_expr(&value.value)
}

fn ensures(task: &Task) -> Vec<&Expression> {
    task.body
        .iter()
        .filter_map(|item| match item {
            Statement::Ensure { value, .. } => Some(value),
            _ => None,
        })
        .collect()
}

fn render_statements(
    statements: &[Statement],
    task: &Task,
    contracts: &[&Expression],
    indent: usize,
    output: &mut Vec<String>,
) {
    let pad = "  ".repeat(indent);
    for statement in statements {
        match statement {
            Statement::Let { name, value, .. } => {
                output.push(format!("{pad}const {name} = {};", render(value)))
            }
            Statement::Show { value, .. } => {
                output.push(format!("{pad}console.log({});", render(value)))
            }
            Statement::Fail { value, .. } => output.push(format!(
                "{pad}throw new AMLFailure(String({}));",
                render(value)
            )),
            Statement::Require { value, .. } => output.push(format!(
                "{pad}__amlRequire(Boolean({}), {}, \"require\", {});",
                render(value),
                quote(&task.name),
                quote(&value.source)
            )),
            Statement::Ensure { .. } => {}
            Statement::Return { value: None, .. } => output.push(format!("{pad}return;")),
            Statement::Return {
                value: Some(value), ..
            } => {
                if contracts.is_empty() && task.result.is_none() {
                    output.push(format!("{pad}return {};", render(value)));
                } else {
                    output.push(format!("{pad}const result = {};", render(value)));
                    for condition in contracts {
                        output.push(format!(
                            "{pad}__amlRequire(Boolean({}), {}, \"ensure\", {});",
                            render(condition),
                            quote(&task.name),
                            quote(&condition.source)
                        ));
                    }
                    if let Some(result) = &task.result {
                        output.push(format!(
                            "{pad}__amlExpect({}, result, {});",
                            quote(&format!("{}.result", task.name)),
                            quote(&result.canonical())
                        ));
                    }
                    output.push(format!("{pad}return result;"));
                }
            }
            Statement::If {
                condition,
                then_body,
                else_body,
                ..
            } => {
                output.push(format!("{pad}if ({}) {{", render(condition)));
                render_statements(then_body, task, contracts, indent + 1, output);
                output.push(format!("{pad}}}"));
                if !else_body.is_empty() {
                    output.last_mut().unwrap().push_str(" else {");
                    render_statements(else_body, task, contracts, indent + 1, output);
                    output.push(format!("{pad}}}"));
                }
            }
            Statement::Expression { value, .. } => output.push(format!("{pad}{};", render(value))),
        }
    }
}

fn records_json(program: &Program) -> String {
    let entries = program
        .records
        .iter()
        .map(|record| {
            let fields = record
                .fields
                .iter()
                .map(|field| format!("{}: {}", quote(&field.name), quote(&field.kind.canonical())))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{}: {{{fields}}}", quote(&record.name))
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("{{{entries}}}")
}

fn contracts_json(program: &Program) -> String {
    let entries = program
        .tasks
        .iter()
        .map(|task| {
            let mut requires = Vec::new();
            let mut ensures = Vec::new();
            for statement in &task.body {
                match statement {
                    Statement::Require { value, .. } => requires.push(quote(&value.source)),
                    Statement::Ensure { value, .. } => ensures.push(quote(&value.source)),
                    _ => {}
                }
            }
            format!(
                "{}: {{\"ensure\": [{}], \"require\": [{}]}}",
                quote(&task.name),
                ensures.join(", "),
                requires.join(", ")
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!("{{{entries}}}")
}

fn capabilities(profile: &str) -> &'static [&'static str] {
    match profile {
        "script" => &["console"],
        "local" => &["console", "files"],
        "connected" => &["console", "files", "network"],
        _ => &[],
    }
}

pub fn compile(program: &Program) -> String {
    let capability_json = capabilities(&program.profile)
        .iter()
        .map(|item| quote(item))
        .collect::<Vec<_>>()
        .join(", ");
    let mut output = vec![
        format!(
            "// Generated by AML {}. Do not edit generated code.",
            crate::VERSION
        ),
        "\"use strict\";".into(),
        String::new(),
        format!("const __amlApp = {};", quote(&program.app)),
        format!("const __amlProfile = {};", quote(&program.profile)),
        format!("const __amlCapabilities = [{capability_json}];"),
        String::new(),
        RUNTIME.into(),
        String::new(),
    ];
    for task in &program.tasks {
        output.push(format!(
            "function {}({}) {{",
            task.name,
            task.parameters
                .iter()
                .map(|parameter| parameter.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
        for parameter in &task.parameters {
            output.push(format!(
                "  __amlExpect({}, {}, {});",
                quote(&format!("{}.{}", task.name, parameter.name)),
                parameter.name,
                quote(&parameter.kind.canonical())
            ));
        }
        let task_ensures = ensures(task);
        render_statements(&task.body, task, &task_ensures, 1, &mut output);
        output.push("}".into());
        output.push(String::new());
    }
    output.push(format!("const __amlRecords = {};", records_json(program)));
    output.push(format!(
        "const __amlContracts = {};",
        contracts_json(program)
    ));
    if program.tasks.iter().any(|task| task.name == "main") {
        output.extend([
            String::new(),
            "try {".into(),
            "  main();".into(),
            "} catch (error) {".into(),
            "  if (error instanceof AMLFailure) {".into(),
            "    console.error(`AML_ERROR: ${error.message}`);".into(),
            "    process.exitCode = 1;".into(),
            "  } else {".into(),
            "    throw error;".into(),
            "  }".into(),
            "}".into(),
        ]);
    }
    output.push(String::new());
    output.join("\n")
}

pub fn compile_source(source: &str) -> Result<String, crate::CompileError> {
    Ok(compile(&crate::core::parse(source)?))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };
    #[test]
    fn generates_typed_javascript() {
        let source = "app score\nprofile script\ntask classify(score: Int) -> Text:\n  require score >= 0\n  ensure result != \"\"\n  if score >= 80:\n    return \"ready\"\n  else:\n    return \"learning\"\ntask main:\n  show classify(90)\n";
        let generated = compile_source(source).unwrap();
        assert!(generated.contains("function classify(score)"));
        assert!(generated.contains("score >= 0"));
        assert!(generated.contains("result !== \"\""));
    }

    #[test]
    fn generated_program_runs_with_node() {
        if Command::new("node").arg("--version").output().is_err() {
            return;
        }
        let source = "app hello\nprofile script\ntask greet(name: Text) -> Text:\n  return \"Hello, \" + name\ntask main:\n  show greet(\"Rust\")\n";
        let generated = compile_source(source).unwrap();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("aml-rust-test-{nonce}.js"));
        fs::write(&path, generated).unwrap();
        let result = Command::new("node").arg(&path).output().unwrap();
        let _ = fs::remove_file(path);
        assert!(result.status.success());
        assert_eq!(String::from_utf8(result.stdout).unwrap(), "Hello, Rust\n");
    }

    #[test]
    fn contract_failure_is_structured() {
        if Command::new("node").arg("--version").output().is_err() {
            return;
        }
        let source = "app invalid\nprofile script\ntask positive(value: Int) -> Int:\n  require value > 0\n  return value\ntask main:\n  show positive(-1)\n";
        let generated = compile_source(source).unwrap();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("aml-rust-contract-{nonce}.js"));
        fs::write(&path, generated).unwrap();
        let result = Command::new("node").arg(&path).output().unwrap();
        let _ = fs::remove_file(path);
        assert!(!result.status.success());
        assert!(String::from_utf8(result.stderr)
            .unwrap()
            .contains("AML_ERROR: contract | positive require failed: value > 0"));
    }
}
