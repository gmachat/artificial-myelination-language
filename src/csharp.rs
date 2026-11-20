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
        "Float" => "double".into(),
        "Int" => "long".into(),
        "Text" => "string".into(),
        "List" => format!("List<{}>", target_type(&value.arguments[0])),
        "Optional" => format!("{}?", target_type(&value.arguments[0])),
        _ => value.name.clone(),
    }
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
        return if actual.name == "None" {
            "null".into()
        } else {
            rendered
        };
    }
    if actual.name == "Int" && expected.name == "Float" {
        return format!("((double) {rendered})");
    }
    rendered
}

fn expression(
    value: &Expr,
    analyzer: &Analyzer,
    env: &HashMap<String, TypeRef>,
    program: &Program,
) -> String {
    match value {
        Expr::Literal(Literal::Int(value)) => value.to_string(),
        Expr::Literal(Literal::Float(value)) => value.to_string(),
        Expr::Literal(Literal::Text(value)) => quote(value),
        Expr::Literal(Literal::Bool(value)) => value.to_string(),
        Expr::Literal(Literal::None) => "null".into(),
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
            if left_type.name == "Text" && right_type.name == "Text" {
                if operator == "+" {
                    return format!("({left_value} + {right_value})");
                }
                let comparison = match operator.as_str() {
                    "<" => "< 0",
                    "<=" => "<= 0",
                    ">" => "> 0",
                    ">=" => ">= 0",
                    _ => "",
                };
                if !comparison.is_empty() {
                    return format!("(string.Compare({left_value}, {right_value}, StringComparison.Ordinal) {comparison})");
                }
            }
            if operator == "/" || left_type.name == "Float" || right_type.name == "Float" {
                let float = TypeRef {
                    name: "Float".into(),
                    arguments: vec![],
                };
                left_value = coerce(left_value, &left_type, &float);
                right_value = coerce(right_value, &right_type, &float);
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
        Expr::Index { value, index } => format!(
            "{}[(int) {}]",
            expression(value, analyzer, env, program),
            expression(index, analyzer, env, program)
        ),
        Expr::Member { value, name } => {
            format!("{}.{name}", expression(value, analyzer, env, program))
        }
        Expr::List(items) => {
            let kind = infer(value, analyzer, env);
            let item_type = &kind.arguments[0];
            let values = items
                .iter()
                .map(|item| {
                    let actual = infer(item, analyzer, env);
                    coerce(expression(item, analyzer, env, program), &actual, item_type)
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("new {} {{ {values} }}", target_type(&kind))
        }
        Expr::Record(fields) => {
            let kind = infer(value, analyzer, env);
            let supplied: BTreeMap<&str, &Expr> = fields
                .iter()
                .map(|(name, value)| (name.as_str(), value))
                .collect();
            let record = program
                .records
                .iter()
                .find(|item| item.name == kind.name)
                .unwrap();
            let values = record
                .fields
                .iter()
                .map(|field| {
                    let value = supplied[field.name.as_str()];
                    let actual = infer(value, analyzer, env);
                    coerce(
                        expression(value, analyzer, env, program),
                        &actual,
                        &field.kind,
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!("new {}({values})", record.name)
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
                output.push(format!(
                    "{pad}{} {name} = {};",
                    target_type(&kind),
                    render(value, analyzer, env, program)
                ));
                env.insert(name.clone(), kind);
            }
            Statement::Show { value, .. } => output.push(format!(
                "{pad}Console.WriteLine({});",
                render(value, analyzer, env, program)
            )),
            Statement::Fail { value, .. } => output.push(format!(
                "{pad}throw new Exception({});",
                render(value, analyzer, env, program)
            )),
            Statement::Require { value, .. } => output.push(format!(
                "{pad}if (!({})) throw new Exception({});",
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
                if contracts.is_empty() {
                    output.push(format!("{pad}return {rendered};"));
                } else {
                    let result_type = task.result.clone().unwrap_or(actual);
                    output.push(format!(
                        "{pad}{} result = {rendered};",
                        target_type(&result_type)
                    ));
                    let mut local = env.clone();
                    local.insert("result".into(), result_type);
                    for condition in contracts {
                        output.push(format!(
                            "{pad}if (!({})) throw new Exception({});",
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
                    "{pad}if ({}) {{",
                    render(condition, analyzer, env, program)
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
        "using System;".into(),
        "using System.Collections.Generic;".into(),
        String::new(),
        "class AMLProgram {".into(),
    ];
    for record in &program.records {
        let fields = record
            .fields
            .iter()
            .map(|field| format!("{} {}", target_type(&field.kind), field.name))
            .collect::<Vec<_>>()
            .join(", ");
        output.extend([
            format!("    record {}({fields});", record.name),
            String::new(),
        ]);
    }
    for task in &program.tasks {
        let parameters = task
            .parameters
            .iter()
            .map(|item| format!("{} {}", target_type(&item.kind), item.name))
            .collect::<Vec<_>>()
            .join(", ");
        let result = task
            .result
            .as_ref()
            .map(target_type)
            .unwrap_or_else(|| "void".into());
        output.push(format!(
            "    static {result} aml_{}({parameters}) {{",
            task.name
        ));
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
            2,
            &mut output,
        );
        output.extend(["    }".into(), String::new()]);
    }
    if program.tasks.iter().any(|task| task.name == "main") {
        output.extend([
            "    public static void Main(string[] args) {".into(),
            "        aml_main();".into(),
            "    }".into(),
        ]);
    }
    output.extend(["}".into(), String::new()]);
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

    fn dotnet_available() -> bool {
        Command::new("dotnet")
            .arg("--version")
            .output()
            .is_ok_and(|result| result.status.success())
    }

    fn run(source: &str) -> String {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root =
            std::env::temp_dir().join(format!("aml-csharp-test-{}-{nonce}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::write(root.join("Program.cs"), compile_source(source).unwrap()).unwrap();
        fs::write(
            root.join("AMLProgram.csproj"),
            "<Project Sdk=\"Microsoft.NET.Sdk\"><PropertyGroup><OutputType>Exe</OutputType><TargetFramework>net8.0</TargetFramework><Nullable>enable</Nullable></PropertyGroup></Project>",
        )
        .unwrap();
        let result = Command::new("dotnet")
            .args(["run", "--project"])
            .arg(root.join("AMLProgram.csproj"))
            .arg("--nologo")
            .output()
            .unwrap();
        let _ = fs::remove_dir_all(root);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        String::from_utf8(result.stdout)
            .unwrap()
            .replace("\r\n", "\n")
    }

    #[test]
    fn generates_csharp_contracts() {
        let generated = compile_source("app demo\nprofile script\ntask classify(score: Int) -> Text:\n  require score >= 0\n  ensure result != \"\"\n  return \"ready\"\ntask main:\n  show classify(92)\n").unwrap();
        assert!(generated.contains("class AMLProgram"));
        assert!(generated.contains("throw new Exception"));
        assert!(generated.contains("public static void Main"));
    }

    #[test]
    fn generates_csharp_structures() {
        let generated = compile_source("app demo\nprofile script\nrecord Person:\n  name: Text\n  names: List[Text]\n  note: Optional[Text]\ntask label(person: Person) -> Text:\n  if person.note != none:\n    return person.name\n  else:\n    return person.names[0]\ntask main:\n  show label({name: \"Ada\", names: [\"A\"], note: \"ready\"})\n").unwrap();
        assert!(generated.contains("record Person(string name, List<string> names, string? note)"));
        assert!(generated.contains("new Person(\"Ada\", new List<string> { \"A\" }, \"ready\")"));
        assert!(generated.contains("person.note != null"));
        assert!(generated.contains("person.names[(int) 0]"));
        if dotnet_available() {
            assert_eq!(run("app demo\nprofile script\nrecord Person:\n  name: Text\n  names: List[Text]\n  note: Optional[Text]\ntask label(person: Person) -> Text:\n  if person.note != none:\n    return person.name\n  else:\n    return person.names[0]\ntask main:\n  show label({name: \"Ada\", names: [\"A\"], note: \"ready\"})\n  show label({name: \"Bea\", names: [\"B\"], note: none})\n"), "Ada\nB\n");
        }
    }
}
