use std::collections::HashSet;

use crate::{
    core::{Expression, Program, Statement, Task},
    expression::{Expr, Literal},
};

const RUNTIME: &str = r#"type AMLFailure struct { Message string }

func amlValid(value any, expected string) bool {
	if expected == "Any" { return true }
	if strings.HasPrefix(expected, "Optional[") {
		return value == nil || amlValid(value, expected[9:len(expected)-1])
	}
	if strings.HasPrefix(expected, "List[") {
		items, ok := value.([]any)
		if !ok { return false }
		inner := expected[5:len(expected)-1]
		for _, item := range items { if !amlValid(item, inner) { return false } }
		return true
	}
	if fields, ok := amlRecords[expected]; ok {
		record, valid := value.(map[string]any)
		if !valid || len(record) != len(fields) { return false }
		for name, fieldType := range fields {
			field, exists := record[name]
			if !exists || !amlValid(field, fieldType) { return false }
		}
		return true
	}
	switch expected {
	case "Bool": _, ok := value.(bool); return ok
	case "Float": number, ok := value.(float64); return ok && !math.IsNaN(number) && !math.IsInf(number, 0)
	case "Int": _, ok := value.(int64); return ok
	case "Text": _, ok := value.(string); return ok
	}
	return false
}

func amlExpect(place string, value any, expected string) {
	if !amlValid(value, expected) {
		panic(AMLFailure{fmt.Sprintf("type | %s expected %s, got %T", place, expected, value)})
	}
}

func amlBool(value any) bool {
	result, ok := value.(bool)
	if !ok { panic(AMLFailure{fmt.Sprintf("condition expected Bool, got %T", value)}) }
	return result
}

func amlNumber(value any) (float64, bool) {
	switch number := value.(type) {
	case int64: return float64(number), true
	case float64: return number, true
	default: return 0, false
	}
}

func amlBinary(operator string, left any, right any) any {
	if operator == "==" { return reflect.DeepEqual(left, right) }
	if operator == "!=" { return !reflect.DeepEqual(left, right) }
	if operator == "and" { return amlBool(left) && amlBool(right) }
	if operator == "or" { return amlBool(left) || amlBool(right) }
	if operator == "+" {
		if a, ok := left.(string); ok { if b, valid := right.(string); valid { return a + b } }
	}
	if a, ok := left.(int64); ok {
		if b, valid := right.(int64); valid {
			switch operator {
			case "+": return a + b
			case "-": return a - b
			case "*": return a * b
			case "%": return a % b
			}
		}
	}
	a, leftNumber := amlNumber(left)
	b, rightNumber := amlNumber(right)
	if leftNumber && rightNumber {
		switch operator {
		case "+": return a + b
		case "-": return a - b
		case "*": return a * b
		case "/": return a / b
		case "<": return a < b
		case "<=": return a <= b
		case ">": return a > b
		case ">=": return a >= b
		}
	}
	if a, ok := left.(string); ok {
		if b, valid := right.(string); valid {
			switch operator {
			case "<": return a < b
			case "<=": return a <= b
			case ">": return a > b
			case ">=": return a >= b
			}
		}
	}
	panic(AMLFailure{fmt.Sprintf("operator %s does not support %T and %T", operator, left, right)})
}

func amlUnary(operator string, value any) any {
	if operator == "not" { return !amlBool(value) }
	if number, ok := value.(int64); ok {
		if operator == "-" { return -number }
		if operator == "+" { return number }
	}
	if number, ok := value.(float64); ok {
		if operator == "-" { return -number }
		if operator == "+" { return number }
	}
	panic(AMLFailure{fmt.Sprintf("unary %s does not support %T", operator, value)})
}

func amlIndex(value any, index any) any {
	if items, ok := value.([]any); ok {
		position, valid := index.(int64)
		if !valid || position < 0 || position >= int64(len(items)) { panic(AMLFailure{"invalid list index"}) }
		return items[position]
	}
	if record, ok := value.(map[string]any); ok {
		name, valid := index.(string)
		if !valid { panic(AMLFailure{"record index must be Text"}) }
		result, exists := record[name]
		if !exists { panic(AMLFailure{"unknown record field: " + name}) }
		return result
	}
	panic(AMLFailure{fmt.Sprintf("cannot index %T", value)})
}

func amlRequire(condition any, task string, kind string) {
	if !amlBool(condition) { panic(AMLFailure{fmt.Sprintf("contract | %s %s failed", task, kind)}) }
}"#;

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

fn expression(value: &Expr, tasks: &HashSet<String>) -> String {
    match value {
        Expr::Literal(Literal::Int(value)) => format!("int64({value})"),
        Expr::Literal(Literal::Float(value)) => format!("float64({value})"),
        Expr::Literal(Literal::Text(value)) => quote(value),
        Expr::Literal(Literal::Bool(value)) => value.to_string(),
        Expr::Literal(Literal::None) => "nil".into(),
        Expr::Name(value) if tasks.contains(value) => format!("amlTask_{value}"),
        Expr::Name(value) => value.clone(),
        Expr::Unary { operator, value } => format!(
            "amlUnary({}, {})",
            quote(operator),
            expression(value, tasks)
        ),
        Expr::Binary {
            left,
            operator,
            right,
        } => format!(
            "amlBinary({}, {}, {})",
            quote(operator),
            expression(left, tasks),
            expression(right, tasks)
        ),
        Expr::Call { callee, arguments } => format!(
            "{}({})",
            expression(callee, tasks),
            arguments
                .iter()
                .map(|item| expression(item, tasks))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Expr::Index { value, index } => format!(
            "amlIndex({}, {})",
            expression(value, tasks),
            expression(index, tasks)
        ),
        Expr::Member { value, name } => {
            format!("amlIndex({}, {})", expression(value, tasks), quote(name))
        }
        Expr::List(items) => format!(
            "[]any{{{}}}",
            items
                .iter()
                .map(|item| expression(item, tasks))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Expr::Record(fields) => format!(
            "map[string]any{{{}}}",
            fields
                .iter()
                .map(|(name, value)| format!("{}: {}", quote(name), expression(value, tasks)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

fn render(value: &Expression, tasks: &HashSet<String>) -> String {
    expression(&value.value, tasks)
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
    tasks: &HashSet<String>,
    contracts: &[&Expression],
    level: usize,
    output: &mut Vec<String>,
) {
    let pad = "\t".repeat(level);
    for statement in statements {
        match statement {
            Statement::Let { name, value, .. } => {
                output.push(format!("{pad}{name} := {}", render(value, tasks)))
            }
            Statement::Show { value, .. } => {
                output.push(format!("{pad}fmt.Println({})", render(value, tasks)))
            }
            Statement::Fail { value, .. } => output.push(format!(
                "{pad}panic(AMLFailure{{fmt.Sprint({})}})",
                render(value, tasks)
            )),
            Statement::Require { value, .. } => output.push(format!(
                "{pad}amlRequire({}, {}, \"require\")",
                render(value, tasks),
                quote(&task.name)
            )),
            Statement::Ensure { .. } => {}
            Statement::Return { value: None, .. } => output.push(format!("{pad}return nil")),
            Statement::Return {
                value: Some(value), ..
            } => {
                let value = render(value, tasks);
                if contracts.is_empty() && task.result.is_none() {
                    output.push(format!("{pad}return {value}"));
                } else {
                    output.push(format!("{pad}result := {value}"));
                    for condition in contracts {
                        output.push(format!(
                            "{pad}amlRequire({}, {}, \"ensure\")",
                            render(condition, tasks),
                            quote(&task.name)
                        ));
                    }
                    if let Some(result) = &task.result {
                        output.push(format!(
                            "{pad}amlExpect({}, result, {})",
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
                output.push(format!("{pad}if amlBool({}) {{", render(condition, tasks)));
                body(then_body, task, tasks, contracts, level + 1, output);
                if !else_body.is_empty() {
                    output.push(format!("{pad}}} else {{"));
                    body(else_body, task, tasks, contracts, level + 1, output);
                }
                output.push(format!("{pad}}}"));
            }
            Statement::Expression { value, .. } => {
                output.push(format!("{pad}{}", render(value, tasks)))
            }
        }
    }
}

fn records(program: &Program) -> Vec<String> {
    let mut output = vec!["var amlRecords = map[string]map[string]string{".into()];
    for record in &program.records {
        let fields = record
            .fields
            .iter()
            .map(|field| format!("{}: {}", quote(&field.name), quote(&field.kind.canonical())))
            .collect::<Vec<_>>()
            .join(", ");
        output.push(format!("\t{}: {{{fields}}},", quote(&record.name)));
    }
    output.push("}".into());
    output
}

pub fn compile(program: &Program) -> String {
    let tasks: HashSet<String> = program.tasks.iter().map(|task| task.name.clone()).collect();
    let mut output = vec![
        format!(
            "// Generated by AML {}. Do not edit generated code.",
            crate::VERSION
        ),
        "package main".into(),
        String::new(),
        "import (\"fmt\"; \"math\"; \"os\"; \"reflect\"; \"strings\")".into(),
        String::new(),
    ];
    output.extend(records(program));
    output.extend([String::new(), RUNTIME.into(), String::new()]);
    for task in &program.tasks {
        output.push(format!(
            "func amlTask_{}({}) any {{",
            task.name,
            task.parameters
                .iter()
                .map(|parameter| format!("{} any", parameter.name))
                .collect::<Vec<_>>()
                .join(", ")
        ));
        for parameter in &task.parameters {
            output.push(format!(
                "\tamlExpect({}, {}, {})",
                quote(&format!("{}.{}", task.name, parameter.name)),
                parameter.name,
                quote(&parameter.kind.canonical())
            ));
        }
        let contracts = ensures(task);
        body(&task.body, task, &tasks, &contracts, 1, &mut output);
        output.extend(["\treturn nil".into(), "}".into(), String::new()]);
    }
    if program.tasks.iter().any(|task| task.name == "main") {
        output.extend([
            "func main() {".into(),
            "\tdefer func() {".into(),
            "\t\tif recovered := recover(); recovered != nil {".into(),
            "\t\t\tif failure, ok := recovered.(AMLFailure); ok { fmt.Fprintln(os.Stderr, \"AML_ERROR:\", failure.Message); os.Exit(1) }".into(),
            "\t\t\tpanic(recovered)".into(),
            "\t\t}".into(),
            "\t}()".into(),
            "\tamlTask_main()".into(),
            "}".into(),
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
    fn generates_and_runs_go() {
        if Command::new("go").arg("version").output().is_err() {
            return;
        }
        let source = "app hello\nprofile script\ntask greet(name: Text) -> Text:\n  return \"Hello, \" + name\ntask main:\n  show greet(\"Go\")\n";
        let generated = compile_source(source).unwrap();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("aml-rust-go-{nonce}"));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("main.go");
        fs::write(&path, generated).unwrap();
        let result = Command::new("go")
            .arg("run")
            .arg(&path)
            .env("GOCACHE", root.join("cache"))
            .output()
            .unwrap();
        let _ = fs::remove_dir_all(root);
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert_eq!(String::from_utf8(result.stdout).unwrap(), "Hello, Go\n");
    }

    #[test]
    fn go_contract_failure_is_structured() {
        if Command::new("go").arg("version").output().is_err() {
            return;
        }
        let source = "app invalid\nprofile script\ntask positive(value: Int) -> Int:\n  require value > 0\n  return value\ntask main:\n  show positive(-1)\n";
        let generated = compile_source(source).unwrap();
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let root = std::env::temp_dir().join(format!("aml-rust-go-contract-{nonce}"));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("main.go");
        fs::write(&path, generated).unwrap();
        let result = Command::new("go")
            .arg("run")
            .arg(&path)
            .env("GOCACHE", root.join("cache"))
            .output()
            .unwrap();
        let _ = fs::remove_dir_all(root);
        assert!(!result.status.success());
        assert!(String::from_utf8(result.stderr)
            .unwrap()
            .contains("AML_ERROR: contract | positive require failed"));
    }
}
