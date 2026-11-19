use std::collections::{BTreeMap, HashMap};

use crate::{
    core::{Expression, Program, Statement, Task},
    expression::{Expr, Literal},
    semantic::{Analyzer, TaskSignature},
    types::TypeRef,
    CompileError,
};

fn quote(value: &str) -> String {
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

fn target_type(value: &TypeRef) -> String {
    match value.name.as_str() {
        "Bool" => "bool".into(),
        "Float" => "f64".into(),
        "Int" => "i64".into(),
        "Text" => "String".into(),
        "List" => format!("Vec<{}>", target_type(&value.arguments[0])),
        "Optional" => format!("Option<{}>", target_type(&value.arguments[0])),
        _ => value.name.clone(),
    }
}

fn copy_type(value: &TypeRef) -> bool {
    matches!(value.name.as_str(), "Bool" | "Float" | "Int")
}

fn analyzer(program: &Program) -> Analyzer {
    let mut analyzer = Analyzer::default();
    for record in &program.records {
        analyzer.records.insert(
            record.name.clone(),
            record
                .fields
                .iter()
                .map(|field| (field.name.clone(), field.kind.clone()))
                .collect(),
        );
    }
    for task in &program.tasks {
        analyzer.tasks.insert(
            task.name.clone(),
            TaskSignature {
                parameters: task
                    .parameters
                    .iter()
                    .map(|item| item.kind.clone())
                    .collect(),
                result: task.result.clone(),
            },
        );
    }
    analyzer
}

fn infer(value: &Expr, analyzer: &Analyzer, env: &HashMap<String, TypeRef>) -> TypeRef {
    analyzer.infer(value, env, false).unwrap()
}

fn coerce(rendered: String, actual: &TypeRef, expected: &TypeRef) -> String {
    if expected.name == "Optional" {
        if actual.name == "None" {
            return "None".into();
        }
        if expected.arguments.first() == Some(actual) {
            return format!("Some({rendered})");
        }
    }
    if actual.name == "Int" && expected.name == "Float" {
        return format!("({rendered} as f64)");
    }
    rendered
}

fn strip_outer_parentheses(rendered: String) -> String {
    rendered
        .strip_prefix('(')
        .and_then(|inner| inner.strip_suffix(')'))
        .unwrap_or(&rendered)
        .to_string()
}

fn expression(
    value: &Expr,
    analyzer: &Analyzer,
    env: &HashMap<String, TypeRef>,
    program: &Program,
) -> String {
    match value {
        Expr::Literal(Literal::Int(value)) => value.to_string(),
        Expr::Literal(Literal::Float(value)) => {
            let value = value.to_string();
            if value.contains('.') {
                value
            } else {
                format!("{value}.0")
            }
        }
        Expr::Literal(Literal::Text(value)) => format!("{}.to_string()", quote(value)),
        Expr::Literal(Literal::Bool(value)) => value.to_string(),
        Expr::Literal(Literal::None) => "None".into(),
        Expr::Name(value) => value.clone(),
        Expr::Unary { operator, value } => format!(
            "({}{})",
            if operator == "not" { "!" } else { operator },
            expression(value, analyzer, env, program)
        ),
        Expr::Binary {
            left,
            operator,
            right,
        } => {
            let left_type = infer(left, analyzer, env);
            let right_type = infer(right, analyzer, env);
            let mut left_value = expression(left, analyzer, env, program);
            let mut right_value = expression(right, analyzer, env, program);
            if left_type.name == "Optional" && right_type.name == "None" {
                right_value = coerce(right_value, &right_type, &left_type);
            }
            if right_type.name == "Optional" && left_type.name == "None" {
                left_value = coerce(left_value, &left_type, &right_type);
            }
            if operator == "+" && left_type.name == "Text" && right_type.name == "Text" {
                return format!("format!(\"{{}}{{}}\", {left_value}, {right_value})");
            }
            if operator == "/" || left_type.name == "Float" || right_type.name == "Float" {
                left_value = coerce(
                    left_value,
                    &left_type,
                    &TypeRef {
                        name: "Float".into(),
                        arguments: vec![],
                    },
                );
                right_value = coerce(
                    right_value,
                    &right_type,
                    &TypeRef {
                        name: "Float".into(),
                        arguments: vec![],
                    },
                );
            }
            let operator = match operator.as_str() {
                "and" => "&&",
                "or" => "||",
                other => other,
            };
            format!("({left_value} {operator} {right_value})")
        }
        Expr::Call { callee, arguments } => {
            let Expr::Name(name) = callee.as_ref() else {
                unreachable!()
            };
            let signature = &analyzer.tasks[name];
            let arguments = arguments
                .iter()
                .zip(&signature.parameters)
                .map(|(argument, expected)| {
                    let actual = infer(argument, analyzer, env);
                    coerce(
                        expression(argument, analyzer, env, program),
                        &actual,
                        expected,
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("aml_{name}({arguments})")
        }
        Expr::Index { value, index } => {
            let result_type = infer(value, analyzer, env);
            let item_type = if result_type.name == "List" {
                result_type.arguments[0].clone()
            } else {
                TypeRef {
                    name: "Text".into(),
                    arguments: vec![],
                }
            };
            let rendered = format!(
                "{}[({}) as usize]",
                expression(value, analyzer, env, program),
                expression(index, analyzer, env, program)
            );
            if copy_type(&item_type) {
                rendered
            } else {
                format!("{rendered}.clone()")
            }
        }
        Expr::Member { value, name } => {
            let owner = infer(value, analyzer, env);
            let field_type = &analyzer.records[&owner.name][name];
            let rendered = format!("{}.{}", expression(value, analyzer, env, program), name);
            if copy_type(field_type) {
                rendered
            } else {
                format!("{rendered}.clone()")
            }
        }
        Expr::List(items) => {
            let list_type = infer(value, analyzer, env);
            let item_type = &list_type.arguments[0];
            format!(
                "vec![{}]",
                items
                    .iter()
                    .map(|item| {
                        let actual = infer(item, analyzer, env);
                        coerce(expression(item, analyzer, env, program), &actual, item_type)
                    })
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
        Expr::Record(fields) => {
            let record_type = infer(value, analyzer, env);
            let supplied: BTreeMap<&str, &Expr> = fields
                .iter()
                .map(|(name, value)| (name.as_str(), value))
                .collect();
            let record = program
                .records
                .iter()
                .find(|record| record.name == record_type.name)
                .unwrap();
            let fields = record
                .fields
                .iter()
                .map(|field| {
                    let value = supplied[field.name.as_str()];
                    let actual = infer(value, analyzer, env);
                    format!(
                        "{}: {}",
                        field.name,
                        coerce(
                            expression(value, analyzer, env, program),
                            &actual,
                            &field.kind
                        )
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("{} {{ {fields} }}", record.name)
        }
    }
}

fn render(
    value: &Expression,
    analyzer: &Analyzer,
    env: &HashMap<String, TypeRef>,
    program: &Program,
) -> String {
    expression(&value.value, analyzer, env, program)
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

#[allow(clippy::too_many_arguments)]
fn body(
    statements: &[Statement],
    task: &Task,
    analyzer: &Analyzer,
    env: &mut HashMap<String, TypeRef>,
    program: &Program,
    contracts: &[&Expression],
    level: usize,
    output: &mut Vec<String>,
) {
    let pad = "    ".repeat(level);
    for statement in statements {
        match statement {
            Statement::Let { name, value, .. } => {
                let kind = infer(&value.value, analyzer, env);
                let rendered = render(value, analyzer, env, program);
                output.push(format!("{pad}let {name} = {rendered};"));
                env.insert(name.clone(), kind);
            }
            Statement::Show { value, .. } => {
                let kind = infer(&value.value, analyzer, env);
                let rendered = render(value, analyzer, env, program);
                output.push(if copy_type(&kind) || kind.name == "Text" {
                    format!("{pad}println!(\"{{}}\", {rendered});")
                } else {
                    format!("{pad}println!(\"{{:?}}\", {rendered});")
                });
            }
            Statement::Fail { value, .. } => output.push(format!(
                "{pad}panic!(\"AML_ERROR: {{}}\", {});",
                render(value, analyzer, env, program)
            )),
            Statement::Require { value, .. } => output.push(format!(
                "{pad}if !({}) {{ panic!({}); }}",
                render(value, analyzer, env, program),
                quote(&format!(
                    "AML_ERROR: contract | {} require failed",
                    task.name
                ))
            )),
            Statement::Ensure { .. } => {}
            Statement::Return { value: None, .. } => output.push(format!("{pad}return;")),
            Statement::Return {
                value: Some(value), ..
            } => {
                let actual = infer(&value.value, analyzer, env);
                let rendered = render(value, analyzer, env, program);
                let rendered = task.result.as_ref().map_or(rendered.clone(), |expected| {
                    coerce(rendered, &actual, expected)
                });
                let rendered = strip_outer_parentheses(rendered);
                if contracts.is_empty() {
                    output.push(format!("{pad}return {rendered};"));
                } else {
                    output.push(format!("{pad}let result = {rendered};"));
                    let mut local = env.clone();
                    local.insert("result".into(), task.result.clone().unwrap_or(actual));
                    for condition in contracts {
                        output.push(format!(
                            "{pad}if !({}) {{ panic!({}); }}",
                            render(condition, analyzer, &local, program),
                            quote(&format!(
                                "AML_ERROR: contract | {} ensure failed",
                                task.name
                            ))
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
                output.push(format!(
                    "{pad}if {} {{",
                    strip_outer_parentheses(render(condition, analyzer, env, program))
                ));
                body(
                    then_body,
                    task,
                    analyzer,
                    &mut env.clone(),
                    program,
                    contracts,
                    level + 1,
                    output,
                );
                if !else_body.is_empty() {
                    output.push(format!("{pad}}} else {{"));
                    body(
                        else_body,
                        task,
                        analyzer,
                        &mut env.clone(),
                        program,
                        contracts,
                        level + 1,
                        output,
                    );
                }
                output.push(format!("{pad}}}"));
            }
            Statement::Expression { value, .. } => {
                output.push(format!("{pad}{};", render(value, analyzer, env, program)))
            }
        }
    }
}

pub fn compile(program: &Program) -> Result<String, CompileError> {
    let analyzer = analyzer(program);
    let mut output = vec![
        format!(
            "// Generated by AML {}. Do not edit generated code.",
            crate::VERSION
        ),
        String::new(),
    ];
    for record in &program.records {
        output.push("#[derive(Debug, Clone, PartialEq)]".into());
        output.push(format!("struct {} {{", record.name));
        for field in &record.fields {
            output.push(format!("    {}: {},", field.name, target_type(&field.kind)));
        }
        output.extend(["}".into(), String::new()]);
    }
    for task in &program.tasks {
        let parameters = task
            .parameters
            .iter()
            .map(|item| format!("{}: {}", item.name, target_type(&item.kind)))
            .collect::<Vec<_>>()
            .join(", ");
        let result = task
            .result
            .as_ref()
            .map(|value| format!(" -> {}", target_type(value)))
            .unwrap_or_default();
        output.push(format!("fn aml_{}({parameters}){result} {{", task.name));
        let mut env = task
            .parameters
            .iter()
            .map(|item| (item.name.clone(), item.kind.clone()))
            .collect();
        let contracts = ensures(task);
        body(
            &task.body,
            task,
            &analyzer,
            &mut env,
            program,
            &contracts,
            1,
            &mut output,
        );
        output.extend(["}".into(), String::new()]);
    }
    if program.tasks.iter().any(|task| task.name == "main") {
        output.extend([
            "fn main() {".into(),
            "    aml_main();".into(),
            "}".into(),
            String::new(),
        ]);
    }
    Ok(output.join("\n"))
}

pub fn compile_source(source: &str) -> Result<String, CompileError> {
    compile(&crate::core::parse(source)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        fs,
        process::Command,
        time::{SystemTime, UNIX_EPOCH},
    };
    fn run(source: &str) -> String {
        if Command::new("rustc").arg("--version").output().is_err() {
            return String::new();
        }
        let generated = compile_source(source).unwrap();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("aml-generated-rust-{nonce}"));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("main.rs");
        let executable = root.join("program");
        fs::write(&path, generated).unwrap();
        let built = Command::new("rustc")
            .arg(&path)
            .arg("-o")
            .arg(&executable)
            .output()
            .unwrap();
        assert!(
            built.status.success(),
            "{}",
            String::from_utf8_lossy(&built.stderr)
        );
        let result = Command::new(&executable).output().unwrap();
        let _ = fs::remove_dir_all(root);
        assert!(result.status.success());
        String::from_utf8(result.stdout).unwrap()
    }
    #[test]
    fn generated_rust_runs() {
        let source = "app native\nprofile script\ntask classify(score: Int) -> Text:\n  if score >= 80:\n    return \"ready\"\n  else:\n    return \"learning\"\ntask main:\n  show classify(92)\n";
        assert_eq!(run(source), "ready\n");
    }
    #[test]
    fn generated_rust_supports_structures() {
        let source = "app structured\nprofile script\nrecord Person:\n  name: Text\n  names: List[Text]\n  note: Optional[Text]\ntask label(person: Person) -> Text:\n  if person.note != none:\n    return person.name\n  else:\n    return person.names[0]\ntask main:\n  show label({name: \"Ada\", names: [\"A\"], note: \"ready\"})\n  show label({name: \"Bea\", names: [\"B\"], note: none})\n";
        assert_eq!(run(source), "Ada\nB\n");
    }
}
