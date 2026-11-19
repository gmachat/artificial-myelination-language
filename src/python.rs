use crate::{
    core::{Expression, Program, Statement, Task},
    expression::{Expr, Literal},
    types::TypeRef,
};

const RUNTIME: &str = r#"class AMLFailure(Exception):
    pass

def __aml_valid(value: object, expected: str) -> bool:
    if expected == "Any": return True
    if expected.startswith("Optional["): return value is None or __aml_valid(value, expected[9:-1])
    if expected.startswith("List["): return isinstance(value, list) and all(__aml_valid(item, expected[5:-1]) for item in value)
    if expected in __aml_records__:
        fields = __aml_records__[expected]
        return isinstance(value, dict) and set(value) == set(fields) and all(__aml_valid(value[name], kind) for name, kind in fields.items())
    scalar = {"Bool": bool, "Float": float, "Int": int, "Text": str}.get(expected)
    return scalar is not None and type(value) is scalar

def __aml_expect(place: str, value: object, expected: str) -> None:
    if not __aml_valid(value, expected):
        display = {"Bool": "bool", "Float": "float", "Int": "int", "Text": "str"}.get(expected, expected)
        raise AMLFailure(f"type | {place} expected {display}, got {type(value).__name__}")

def __aml_require(condition: bool, task: str, kind: str, expression: str) -> None:
    if not condition: raise AMLFailure(f"contract | {task} {kind} failed: {expression}")"#;

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

fn annotation(value: &TypeRef) -> String {
    match value.name.as_str() {
        "Any" => "object".into(),
        "Bool" => "bool".into(),
        "Float" => "float".into(),
        "Int" => "int".into(),
        "Text" => "str".into(),
        "List" => format!("list[{}]", annotation(&value.arguments[0])),
        "Optional" => format!("{} | None", annotation(&value.arguments[0])),
        _ => "dict[str, object]".into(),
    }
}

fn expression(value: &Expr) -> String {
    match value {
        Expr::Literal(Literal::Int(value)) => value.to_string(),
        Expr::Literal(Literal::Float(value)) => value.to_string(),
        Expr::Literal(Literal::Text(value)) => quote(value),
        Expr::Literal(Literal::Bool(value)) => {
            if *value {
                "True".into()
            } else {
                "False".into()
            }
        }
        Expr::Literal(Literal::None) => "None".into(),
        Expr::Name(value) => value.clone(),
        Expr::Unary { operator, value } => {
            if operator == "not" {
                format!("(not {})", expression(value))
            } else {
                format!("({operator}{})", expression(value))
            }
        }
        Expr::Binary {
            left,
            operator,
            right,
        } => format!("({} {operator} {})", expression(left), expression(right)),
        Expr::Call { callee, arguments } => format!(
            "{}({})",
            expression(callee),
            arguments
                .iter()
                .map(expression)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Expr::Index { value, index } => format!("{}[{}]", expression(value), expression(index)),
        Expr::Member { value, name } => format!("{}[{}]", expression(value), quote(name)),
        Expr::List(items) => format!(
            "[{}]",
            items.iter().map(expression).collect::<Vec<_>>().join(", ")
        ),
        Expr::Record(fields) => format!(
            "{{{}}}",
            fields
                .iter()
                .map(|(name, value)| format!("{}: {}", quote(name), expression(value)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn render(value: &Expression) -> String {
    expression(&value.value)
}
fn ensures(task: &Task) -> Vec<&Expression> {
    task.body
        .iter()
        .filter_map(|statement| match statement {
            Statement::Ensure { value, .. } => Some(value),
            _ => None,
        })
        .collect()
}

fn body(
    statements: &[Statement],
    task: &Task,
    contracts: &[&Expression],
    level: usize,
    output: &mut Vec<String>,
) {
    let start = output.len();
    let pad = "    ".repeat(level);
    for statement in statements {
        match statement {
            Statement::Let { name, value, .. } => {
                output.push(format!("{pad}{name} = {}", render(value)))
            }
            Statement::Show { value, .. } => output.push(format!("{pad}print({})", render(value))),
            Statement::Fail { value, .. } => {
                output.push(format!("{pad}raise AMLFailure(str({}))", render(value)))
            }
            Statement::Require { value, .. } => output.push(format!(
                "{pad}__aml_require(bool({}), {}, \"require\", {})",
                render(value),
                quote(&task.name),
                quote(&value.source)
            )),
            Statement::Ensure { .. } => {}
            Statement::Return { value: None, .. } => output.push(format!("{pad}return")),
            Statement::Return {
                value: Some(value), ..
            } => {
                if contracts.is_empty() && task.result.is_none() {
                    output.push(format!("{pad}return {}", render(value)));
                } else {
                    output.push(format!("{pad}result = {}", render(value)));
                    for condition in contracts {
                        output.push(format!(
                            "{pad}__aml_require(bool({}), {}, \"ensure\", {})",
                            render(condition),
                            quote(&task.name),
                            quote(&condition.source)
                        ));
                    }
                    if let Some(result) = &task.result {
                        output.push(format!(
                            "{pad}__aml_expect({}, result, {})",
                            quote(&format!("{}.result", task.name)),
                            quote(&result.canonical())
                        ));
                    }
                    output.push(format!("{pad}return result"));
                }
            }
            Statement::If {
                condition,
                then_body,
                else_body,
                ..
            } => {
                output.push(format!("{pad}if {}:", render(condition)));
                body(then_body, task, contracts, level + 1, output);
                if !else_body.is_empty() {
                    output.push(format!("{pad}else:"));
                    body(else_body, task, contracts, level + 1, output);
                }
            }
            Statement::Expression { value, .. } => output.push(format!("{pad}{}", render(value))),
        }
    }
    if output.len() == start {
        output.push(format!("{pad}pass"));
    }
}

fn records(program: &Program) -> String {
    format!(
        "{{{}}}",
        program
            .records
            .iter()
            .map(|record| format!(
                "{}: {{{}}}",
                quote(&record.name),
                record
                    .fields
                    .iter()
                    .map(|field| format!(
                        "{}: {}",
                        quote(&field.name),
                        quote(&field.kind.canonical())
                    ))
                    .collect::<Vec<_>>()
                    .join(", ")
            ))
            .collect::<Vec<_>>()
            .join(", ")
    )
}
fn contracts(program: &Program) -> String {
    format!(
        "{{{}}}",
        program
            .tasks
            .iter()
            .map(|task| {
                let required = task
                    .body
                    .iter()
                    .filter_map(|statement| match statement {
                        Statement::Require { value, .. } => Some(quote(&value.source)),
                        _ => None,
                    })
                    .collect::<Vec<_>>()
                    .join(", ");
                let ensured = ensures(task)
                    .iter()
                    .map(|value| quote(&value.source))
                    .collect::<Vec<_>>()
                    .join(", ");
                format!(
                    "{}: {{\"require\": [{required}], \"ensure\": [{ensured}]}}",
                    quote(&task.name)
                )
            })
            .collect::<Vec<_>>()
            .join(", ")
    )
}

pub fn compile(program: &Program) -> String {
    let capabilities = match program.profile.as_str() {
        "local" => "(\"console\", \"files\")",
        "connected" => "(\"console\", \"files\", \"network\")",
        _ => "(\"console\",)",
    };
    let mut output = vec![
        format!(
            "# Generated by AML {}. Do not edit generated code.",
            crate::VERSION
        ),
        "from __future__ import annotations".into(),
        "import sys".into(),
        String::new(),
        format!("__aml_app__ = {}", quote(&program.app)),
        format!("__aml_profile__ = {}", quote(&program.profile)),
        format!("__aml_capabilities__ = {capabilities}"),
        format!("__aml_records__ = {}", records(program)),
        format!("__aml_contracts__ = {}", contracts(program)),
        String::new(),
        RUNTIME.into(),
        String::new(),
    ];
    for task in &program.tasks {
        let parameters = task
            .parameters
            .iter()
            .map(|parameter| format!("{}: {}", parameter.name, annotation(&parameter.kind)))
            .collect::<Vec<_>>()
            .join(", ");
        let result = task
            .result
            .as_ref()
            .map(annotation)
            .unwrap_or_else(|| "None".into());
        output.push(format!("def {}({parameters}) -> {result}:", task.name));
        for parameter in &task.parameters {
            output.push(format!(
                "    __aml_expect({}, {}, {})",
                quote(&format!("{}.{}", task.name, parameter.name)),
                parameter.name,
                quote(&parameter.kind.canonical())
            ));
        }
        let task_contracts = ensures(task);
        body(&task.body, task, &task_contracts, 1, &mut output);
        output.push(String::new());
    }
    if program.tasks.iter().any(|task| task.name == "main") {
        output.extend([
            "if __name__ == \"__main__\":".into(),
            "    try:".into(),
            "        main()".into(),
            "    except AMLFailure as error:".into(),
            "        print(f\"AML_ERROR: {error}\", file=sys.stderr)".into(),
            "        raise SystemExit(1)".into(),
            String::new(),
        ]);
    }
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
    fn run(source: &str) -> String {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("aml-python-{}-{nonce}.py", std::process::id()));
        fs::write(&path, compile_source(source).unwrap()).unwrap();
        let result = Command::new("python3").arg(&path).output().unwrap();
        let _ = fs::remove_file(path);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        String::from_utf8(result.stdout).unwrap()
    }
    #[test]
    fn generated_python_runs() {
        assert_eq!(run("app demo\nprofile script\ntask greet(name: Text) -> Text:\n  return \"Hello, \" + name\ntask main:\n  show greet(\"AML\")\n"),"Hello, AML\n");
    }
    #[test]
    fn generated_python_supports_structures() {
        assert_eq!(run("app demo\nprofile script\nrecord Person:\n  name: Text\n  scores: List[Int]\ntask first(person: Person) -> Int:\n  return person.scores[0]\ntask main:\n  show first({name: \"Ada\", scores: [42]})\n"),"42\n");
    }
}
