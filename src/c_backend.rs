use std::collections::{BTreeMap, HashMap, HashSet};

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

fn alias(value: &TypeRef) -> String {
    format!(
        "AML_{}",
        value
            .canonical()
            .chars()
            .map(|item| if item.is_ascii_alphanumeric() {
                item
            } else {
                '_'
            })
            .collect::<String>()
    )
}

fn target_type(value: &TypeRef) -> String {
    match value.name.as_str() {
        "Bool" => "bool".into(),
        "Float" => "double".into(),
        "Int" => "long long".into(),
        "Text" => "const char *".into(),
        "List" | "Optional" => alias(value),
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
        let kind = alias(expected);
        if actual.name == "None" {
            return format!("({kind}){{ .has = false }}");
        }
        return format!("({kind}){{ .has = true, .value = {rendered} }}");
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
        Expr::Literal(Literal::None) => "0".into(),
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
            if matches!(operator.as_str(), "==" | "!=") {
                if left_type.name == "Optional" && right_type.name == "None" {
                    let has = format!("({}).has", expression(left, analyzer, env, program));
                    return if operator == "==" {
                        format!("(!{has})")
                    } else {
                        has
                    };
                }
                if right_type.name == "Optional" && left_type.name == "None" {
                    let has = format!("({}).has", expression(right, analyzer, env, program));
                    return if operator == "==" {
                        format!("(!{has})")
                    } else {
                        has
                    };
                }
            }
            let left_value = expression(left, analyzer, env, program);
            let right_value = expression(right, analyzer, env, program);
            if left_type.name == "Text" && right_type.name == "Text" {
                if operator == "+" {
                    return format!("aml_text_add({left_value}, {right_value})");
                }
                let compare = match operator.as_str() {
                    "==" => "== 0",
                    "!=" => "!= 0",
                    "<" => "< 0",
                    "<=" => "<= 0",
                    ">" => "> 0",
                    ">=" => ">= 0",
                    _ => "",
                };
                if !compare.is_empty() {
                    return format!("(strcmp({left_value}, {right_value}) {compare})");
                }
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
            "({}).items[(size_t) ({})]",
            expression(value, analyzer, env, program),
            expression(index, analyzer, env, program)
        ),
        Expr::Member { value, name } => {
            format!("({}).{name}", expression(value, analyzer, env, program))
        }
        Expr::List(items) => {
            let kind = infer(value, analyzer, env);
            let item_type = &kind.arguments[0];
            if items.is_empty() {
                return format!("({}){{ .items = NULL, .len = 0 }}", alias(&kind));
            }
            let values = items
                .iter()
                .map(|item| {
                    let actual = infer(item, analyzer, env);
                    coerce(expression(item, analyzer, env, program), &actual, item_type)
                })
                .collect::<Vec<_>>()
                .join(", ");
            format!(
                "aml_make_{}(({}[]){{{values}}}, {})",
                alias(&kind),
                target_type(item_type),
                items.len()
            )
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
            let fields = record
                .fields
                .iter()
                .map(|field| {
                    let value = supplied[field.name.as_str()];
                    let actual = infer(value, analyzer, env);
                    format!(
                        ".{} = {}",
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
            format!("({}){{ {fields} }}", record.name)
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
            Statement::Show { value, .. } => {
                let kind = infer(&value.value, analyzer, env);
                let value = render(value, analyzer, env, program);
                let line = match kind.name.as_str() {
                    "Text" => format!("printf(\"%s\\n\", {value});"),
                    "Int" => format!("printf(\"%lld\\n\", {value});"),
                    "Float" => format!("printf(\"%g\\n\", {value});"),
                    "Bool" => format!("printf(\"%s\\n\", ({value}) ? \"true\" : \"false\");"),
                    _ => format!("printf(\"<{}>\\n\");", kind.canonical()),
                };
                output.push(format!("{pad}{line}"));
            }
            Statement::Fail { value, .. } => output.push(format!(
                "{pad}aml_fail({});",
                render(value, analyzer, env, program)
            )),
            Statement::Require { value, .. } => output.push(format!(
                "{pad}if (!({})) {{ aml_fail({}); }}",
                render(value, analyzer, env, program),
                quote(&format!("contract | {} require failed", task.name))
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
                            "{pad}if (!({})) {{ aml_fail({}); }}",
                            render(condition, analyzer, &local, program),
                            quote(&format!("contract | {} ensure failed", task.name))
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

fn emit_type(
    kind: &TypeRef,
    program: &Program,
    emitted: &mut HashSet<String>,
    active: &mut HashSet<String>,
    output: &mut Vec<String>,
) -> Result<(), CompileError> {
    let key = kind.canonical();
    if matches!(
        kind.name.as_str(),
        "Bool" | "Float" | "Int" | "Text" | "None"
    ) || emitted.contains(&key)
    {
        return Ok(());
    }
    if !active.insert(key.clone()) {
        return Err(CompileError::new(
            format!("C backend does not yet support recursive type {key}"),
            1,
        ));
    }
    match kind.name.as_str() {
        "List" => {
            let inner = &kind.arguments[0];
            emit_type(inner, program, emitted, active, output)?;
            let name = alias(kind);
            let item = target_type(inner);
            output.push(format!(
                "typedef struct {{ {item} *items; size_t len; }} {name};"
            ));
            output.push(format!(
                "static {name} aml_make_{name}({item} *items, size_t len) {{"
            ));
            output.push(format!(
                "    {name} result = {{ .items = NULL, .len = len }};"
            ));
            output.push("    if (len) {".into());
            output.push(format!(
                "        result.items = malloc(sizeof({item}) * len);"
            ));
            output.push("        if (!result.items) aml_fail(\"out of memory\");".into());
            output.push(format!(
                "        memcpy(result.items, items, sizeof({item}) * len);"
            ));
            output.extend(["    }".into(), "    return result;".into(), "}".into()]);
        }
        "Optional" => {
            let inner = &kind.arguments[0];
            emit_type(inner, program, emitted, active, output)?;
            output.push(format!(
                "typedef struct {{ bool has; {} value; }} {};",
                target_type(inner),
                alias(kind)
            ));
        }
        _ => {
            let record = program
                .records
                .iter()
                .find(|record| record.name == kind.name)
                .ok_or_else(|| {
                    CompileError::new(format!("C backend cannot define type {key}"), 1)
                })?;
            for field in &record.fields {
                emit_type(&field.kind, program, emitted, active, output)?;
            }
            output.push(format!("struct {} {{", record.name));
            for field in &record.fields {
                output.push(format!("    {} {};", target_type(&field.kind), field.name));
            }
            output.push("};".into());
        }
    }
    active.remove(&key);
    emitted.insert(key);
    Ok(())
}

fn collect_expression_types(
    value: &Expr,
    analyzer: &Analyzer,
    env: &HashMap<String, TypeRef>,
    output: &mut HashSet<TypeRef>,
) {
    output.insert(infer(value, analyzer, env));
    match value {
        Expr::Unary { value, .. } => collect_expression_types(value, analyzer, env, output),
        Expr::Binary { left, right, .. } => {
            collect_expression_types(left, analyzer, env, output);
            collect_expression_types(right, analyzer, env, output);
        }
        Expr::Call { arguments, .. } => {
            for argument in arguments {
                collect_expression_types(argument, analyzer, env, output);
            }
        }
        Expr::Index { value, index } => {
            collect_expression_types(value, analyzer, env, output);
            collect_expression_types(index, analyzer, env, output);
        }
        Expr::Member { value, .. } => collect_expression_types(value, analyzer, env, output),
        Expr::List(items) => {
            for item in items {
                collect_expression_types(item, analyzer, env, output);
            }
        }
        Expr::Record(fields) => {
            for (_, value) in fields {
                collect_expression_types(value, analyzer, env, output);
            }
        }
        Expr::Literal(_) | Expr::Name(_) => {}
    }
}

fn collect_statement_types(
    statements: &[Statement],
    analyzer: &Analyzer,
    env: &mut HashMap<String, TypeRef>,
    output: &mut HashSet<TypeRef>,
) {
    for statement in statements {
        let value = match statement {
            Statement::Let { value, .. }
            | Statement::Show { value, .. }
            | Statement::Fail { value, .. }
            | Statement::Require { value, .. }
            | Statement::Expression { value, .. } => Some(value),
            Statement::Ensure { .. } => None,
            Statement::Return { value, .. } => value.as_ref(),
            Statement::If {
                condition,
                then_body,
                else_body,
                ..
            } => {
                collect_expression_types(&condition.value, analyzer, env, output);
                collect_statement_types(then_body, analyzer, &mut env.clone(), output);
                collect_statement_types(else_body, analyzer, &mut env.clone(), output);
                None
            }
        };
        if let Some(value) = value {
            collect_expression_types(&value.value, analyzer, env, output);
        }
        if let Statement::Let { name, value, .. } = statement {
            env.insert(name.clone(), infer(&value.value, analyzer, env));
        }
    }
}

pub fn compile(program: &Program) -> Result<String, CompileError> {
    let analyzer = analyzer(program);
    let mut output = vec![
        format!("/* Generated by AML {}. Do not edit generated code. */", crate::VERSION),
        "#include <stdbool.h>".into(), "#include <stddef.h>".into(), "#include <stdio.h>".into(),
        "#include <stdlib.h>".into(), "#include <string.h>".into(), String::new(),
        "static void aml_fail(const char *message) { fprintf(stderr, \"AML_ERROR: %s\\n\", message); exit(1); }".into(),
        "static char *aml_text_add(const char *a, const char *b) { size_t n = strlen(a) + strlen(b) + 1; char *r = malloc(n); if (!r) aml_fail(\"out of memory\"); strcpy(r, a); strcat(r, b); return r; }".into(),
        String::new(),
    ];
    for record in &program.records {
        output.push(format!("typedef struct {} {};", record.name, record.name));
    }
    let mut emitted = HashSet::new();
    let mut active = HashSet::new();
    for record in &program.records {
        emit_type(
            &TypeRef {
                name: record.name.clone(),
                arguments: vec![],
            },
            program,
            &mut emitted,
            &mut active,
            &mut output,
        )?;
    }
    for task in &program.tasks {
        for parameter in &task.parameters {
            emit_type(
                &parameter.kind,
                program,
                &mut emitted,
                &mut active,
                &mut output,
            )?;
        }
        if let Some(result) = &task.result {
            emit_type(result, program, &mut emitted, &mut active, &mut output)?;
        }
    }
    let mut used_types = HashSet::new();
    for task in &program.tasks {
        let mut env = task
            .parameters
            .iter()
            .map(|item| (item.name.clone(), item.kind.clone()))
            .collect();
        collect_statement_types(&task.body, &analyzer, &mut env, &mut used_types);
    }
    for kind in used_types {
        emit_type(&kind, program, &mut emitted, &mut active, &mut output)?;
    }
    output.push(String::new());
    for task in &program.tasks {
        let parameters = if task.parameters.is_empty() {
            "void".into()
        } else {
            task.parameters
                .iter()
                .map(|item| format!("{} {}", target_type(&item.kind), item.name))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let result = task
            .result
            .as_ref()
            .map(target_type)
            .unwrap_or_else(|| "void".into());
        output.push(format!("{result} aml_{}({parameters});", task.name));
    }
    output.push(String::new());
    for task in &program.tasks {
        let parameters = if task.parameters.is_empty() {
            "void".into()
        } else {
            task.parameters
                .iter()
                .map(|item| format!("{} {}", target_type(&item.kind), item.name))
                .collect::<Vec<_>>()
                .join(", ")
        };
        let result = task
            .result
            .as_ref()
            .map(target_type)
            .unwrap_or_else(|| "void".into());
        output.push(format!("{result} aml_{}({parameters}) {{", task.name));
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
            "int main(void) {".into(),
            "    aml_main();".into(),
            "    return 0;".into(),
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
        sync::atomic::{AtomicU64, Ordering},
        time::{SystemTime, UNIX_EPOCH},
    };

    static NEXT_TEST: AtomicU64 = AtomicU64::new(0);

    fn run(source: &str) -> String {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let sequence = NEXT_TEST.fetch_add(1, Ordering::Relaxed);
        let root = std::env::temp_dir().join(format!(
            "aml-c-test-{}-{nonce}-{sequence}",
            std::process::id()
        ));
        fs::create_dir_all(&root).unwrap();
        let path = root.join("main.c");
        let executable = root.join("program");
        fs::write(&path, compile_source(source).unwrap()).unwrap();
        let compiled = Command::new("cc")
            .args([
                "-std=c11",
                path.to_str().unwrap(),
                "-o",
                executable.to_str().unwrap(),
            ])
            .output()
            .unwrap();
        assert!(
            compiled.status.success(),
            "{}",
            String::from_utf8_lossy(&compiled.stderr)
        );
        let result = Command::new(&executable).output().unwrap();
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
    fn generated_c_runs() {
        assert_eq!(run("app demo\nprofile script\ntask classify(score: Int) -> Text:\n  require score >= 0\n  ensure result != \"\"\n  if score >= 80:\n    return \"ready\"\n  else:\n    return \"wait\"\ntask main:\n  show classify(92)\n"), "ready\n");
    }

    #[test]
    fn generated_c_supports_structures() {
        assert_eq!(run("app demo\nprofile script\nrecord Person:\n  name: Text\n  names: List[Text]\n  note: Optional[Text]\ntask label(person: Person) -> Text:\n  if person.note != none:\n    return person.name\n  else:\n    return person.names[0]\ntask main:\n  show label({name: \"Ada\", names: [\"A\"], note: \"ready\"})\n  show label({name: \"Bea\", names: [\"B\"], note: none})\n"), "Ada\nB\n");
    }

    #[test]
    fn generated_c_defines_local_collection_types() {
        assert_eq!(
            run("app demo\nprofile script\ntask main:\n  let values = [3, 5]\n  show values[1]\n"),
            "5\n"
        );
    }

    #[test]
    fn rejects_recursive_values() {
        let error = compile_source("app demo\nprofile script\nrecord Node:\n  next: Optional[Node]\ntask main:\n  show \"ok\"\n").unwrap_err();
        assert!(error.message.contains("recursive type"));
    }
}
