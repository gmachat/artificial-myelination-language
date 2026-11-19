use crate::{
    core::{Expression, Program, Statement, Task},
    expression::{Expr, Literal},
};

const RUNTIME: &str = r#"class AMLFailure < StandardError
end

def aml_valid(value, expected)
  return true if expected == "Any"
  if expected.start_with?("Optional[")
    return value.nil? || aml_valid(value, expected[9...-1])
  end
  if expected.start_with?("List[")
    return value.is_a?(Array) && value.all? { |item| aml_valid(item, expected[5...-1]) }
  end
  if AML_RECORDS.key?(expected)
    fields = AML_RECORDS[expected]
    return value.is_a?(Hash) && value.keys.sort == fields.keys.sort &&
      fields.all? { |name, type| aml_valid(value[name], type) }
  end
  return value == true || value == false if expected == "Bool"
  return value.instance_of?(Float) if expected == "Float"
  return value.instance_of?(Integer) if expected == "Int"
  return value.instance_of?(String) if expected == "Text"
  false
end

def aml_expect(place, value, expected)
  return if aml_valid(value, expected)
  raise AMLFailure, "type | #{place} expected #{expected}, got #{value.class}"
end

def aml_require(condition, task, kind, expression)
  raise AMLFailure, "contract | #{task} #{kind} failed: #{expression}" unless condition
end"#;

fn quote(value: &str) -> String {
    format!(
        "\"{}\"",
        value
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n")
            .replace('#', "\\#")
    )
}

fn expression(value: &Expr) -> String {
    match value {
        Expr::Literal(Literal::Int(value)) => value.to_string(),
        Expr::Literal(Literal::Float(value)) => value.to_string(),
        Expr::Literal(Literal::Text(value)) => quote(value),
        Expr::Literal(Literal::Bool(value)) => value.to_string(),
        Expr::Literal(Literal::None) => "nil".into(),
        Expr::Name(value) => value.clone(),
        Expr::Unary { operator, value } => {
            let operator = if operator == "not" { "!" } else { operator };
            format!("({operator}{})", expression(value))
        }
        Expr::Binary {
            left,
            operator,
            right,
        } => {
            let operator = match operator.as_str() {
                "and" => "&&",
                "or" => "||",
                other => other,
            };
            format!("({} {operator} {})", expression(left), expression(right))
        }
        Expr::Call { callee, arguments } => format!(
            "{}({})",
            expression(callee),
            arguments
                .iter()
                .map(expression)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Expr::Index { value, index } => {
            format!("{}[{}]", expression(value), expression(index))
        }
        Expr::Member { value, name } => format!("{}[{}]", expression(value), quote(name)),
        Expr::List(items) => format!(
            "[{}]",
            items.iter().map(expression).collect::<Vec<_>>().join(", ")
        ),
        Expr::Record(fields) => format!(
            "{{{}}}",
            fields
                .iter()
                .map(|(name, value)| format!("{} => {}", quote(name), expression(value)))
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
    let pad = "  ".repeat(level);
    for statement in statements {
        match statement {
            Statement::Let { name, value, .. } => {
                output.push(format!("{pad}{name} = {}", render(value)))
            }
            Statement::Show { value, .. } => output.push(format!("{pad}puts({})", render(value))),
            Statement::Fail { value, .. } => {
                output.push(format!("{pad}raise AMLFailure, String({})", render(value)))
            }
            Statement::Require { value, .. } => output.push(format!(
                "{pad}aml_require(!!({}), {}, \"require\", {})",
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
                            "{pad}aml_require(!!({}), {}, \"ensure\", {})",
                            render(condition),
                            quote(&task.name),
                            quote(&condition.source)
                        ));
                    }
                    if let Some(result) = &task.result {
                        output.push(format!(
                            "{pad}aml_expect({}, result, {})",
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
                output.push(format!("{pad}if {}", render(condition)));
                body(then_body, task, contracts, level + 1, output);
                if !else_body.is_empty() {
                    output.push(format!("{pad}else"));
                    body(else_body, task, contracts, level + 1, output);
                }
                output.push(format!("{pad}end"));
            }
            Statement::Expression { value, .. } => output.push(format!("{pad}{}", render(value))),
        }
    }
    if output.len() == start {
        output.push(format!("{pad}nil"));
    }
}

fn records(program: &Program) -> String {
    format!(
        "{{{}}}",
        program
            .records
            .iter()
            .map(|record| format!(
                "{} => {{{}}}",
                quote(&record.name),
                record
                    .fields
                    .iter()
                    .map(|field| format!(
                        "{} => {}",
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

pub fn compile(program: &Program) -> String {
    let mut output = vec![
        format!(
            "# Generated by AML {}. Do not edit generated code.",
            crate::VERSION
        ),
        format!("AML_RECORDS = {}", records(program)),
        String::new(),
        RUNTIME.into(),
        String::new(),
    ];
    for task in &program.tasks {
        output.push(format!(
            "def {}({})",
            task.name,
            task.parameters
                .iter()
                .map(|parameter| parameter.name.as_str())
                .collect::<Vec<_>>()
                .join(", ")
        ));
        for parameter in &task.parameters {
            output.push(format!(
                "  aml_expect({}, {}, {})",
                quote(&format!("{}.{}", task.name, parameter.name)),
                parameter.name,
                quote(&parameter.kind.canonical())
            ));
        }
        let contracts = ensures(task);
        body(&task.body, task, &contracts, 1, &mut output);
        output.extend(["end".into(), String::new()]);
    }
    if program.tasks.iter().any(|task| task.name == "main") {
        output.extend([
            "begin".into(),
            "  main()".into(),
            "rescue AMLFailure => error".into(),
            "  warn \"AML_ERROR: #{error.message}\"".into(),
            "  exit 1".into(),
            "end".into(),
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

    #[test]
    fn generates_and_runs_ruby() {
        if Command::new("ruby").arg("--version").output().is_err() {
            return;
        }
        let source = "app hello\nprofile script\ntask greet(name: Text) -> Text:\n  return \"Hello, \" + name\ntask main:\n  show greet(\"Ruby\")\n";
        let generated = compile_source(source).unwrap();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path = std::env::temp_dir().join(format!("aml-rust-ruby-{nonce}.rb"));
        fs::write(&path, generated).unwrap();
        let result = Command::new("ruby").arg(&path).output().unwrap();
        let _ = fs::remove_file(path);
        assert!(result.status.success());
        assert_eq!(String::from_utf8(result.stdout).unwrap(), "Hello, Ruby\n");
    }
}
