use std::collections::{BTreeMap, HashMap};

use crate::{
    core::{Expression, Program, Statement},
    expression::{Expr, Literal},
    semantic::{Analyzer, TaskSignature},
    types::TypeRef,
    CompileError,
};

#[derive(Clone)]
enum Json {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<Json>),
    Object(BTreeMap<String, Json>),
}

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

impl Json {
    fn pretty(&self, indent: usize) -> String {
        match self {
            Self::Null => "null".into(),
            Self::Bool(value) => value.to_string(),
            Self::Number(value) => value.clone(),
            Self::String(value) => quote(value),
            Self::Array(items) if items.is_empty() => "[]".into(),
            Self::Array(items) => {
                let pad = " ".repeat(indent + 2);
                let body = items
                    .iter()
                    .map(|item| format!("{pad}{}", item.pretty(indent + 2)))
                    .collect::<Vec<_>>()
                    .join(",\n");
                format!("[\n{body}\n{}]", " ".repeat(indent))
            }
            Self::Object(fields) if fields.is_empty() => "{}".into(),
            Self::Object(fields) => {
                let pad = " ".repeat(indent + 2);
                let body = fields
                    .iter()
                    .map(|(name, value)| {
                        format!("{pad}{}: {}", quote(name), value.pretty(indent + 2))
                    })
                    .collect::<Vec<_>>()
                    .join(",\n");
                format!("{{\n{body}\n{}}}", " ".repeat(indent))
            }
        }
    }
}

fn object(fields: impl IntoIterator<Item = (impl Into<String>, Json)>) -> Json {
    Json::Object(
        fields
            .into_iter()
            .map(|(name, value)| (name.into(), value))
            .collect(),
    )
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
                    .map(|parameter| parameter.kind.clone())
                    .collect(),
                result: task.result.clone(),
            },
        );
    }
    analyzer
}

fn expression_json(
    expression: &Expr,
    analyzer: &Analyzer,
    env: &HashMap<String, TypeRef>,
    allow_void: bool,
) -> Result<Json, CompileError> {
    let inferred = analyzer
        .infer(expression, env, allow_void)
        .map_err(|message| CompileError::new(message, 1))?;
    let kind = inferred.canonical();
    Ok(match expression {
        Expr::Literal(value) => {
            let (literal, value) = match value {
                Literal::Int(value) => ("number", Json::Number(value.to_string())),
                Literal::Float(value) => ("number", Json::Number(value.to_string())),
                Literal::Text(value) => ("string", Json::String(value.clone())),
                Literal::Bool(value) => ("constant", Json::Bool(*value)),
                Literal::None => ("constant", Json::Null),
            };
            object([
                ("kind", Json::String("literal".into())),
                ("literal", Json::String(literal.into())),
                ("type", Json::String(kind)),
                ("value", value),
            ])
        }
        Expr::Name(value) => object([
            ("kind", Json::String("name".into())),
            ("type", Json::String(kind)),
            ("value", Json::String(value.clone())),
        ]),
        Expr::Unary { operator, value } => object([
            ("kind", Json::String("unary".into())),
            ("operator", Json::String(operator.clone())),
            ("type", Json::String(kind)),
            ("value", expression_json(value, analyzer, env, false)?),
        ]),
        Expr::Binary {
            left,
            operator,
            right,
        } => object([
            ("kind", Json::String("binary".into())),
            ("left", expression_json(left, analyzer, env, false)?),
            ("operator", Json::String(operator.clone())),
            ("right", expression_json(right, analyzer, env, false)?),
            ("type", Json::String(kind)),
        ]),
        Expr::Call { callee, arguments } => {
            let Expr::Name(name) = callee.as_ref() else {
                unreachable!("semantic analysis rejects non-name callees")
            };
            let signature = &analyzer.tasks[name];
            let result = signature
                .result
                .as_ref()
                .map_or_else(|| "None".into(), TypeRef::canonical);
            let signature_type = format!(
                "Task[{} -> {result}]",
                signature
                    .parameters
                    .iter()
                    .map(TypeRef::canonical)
                    .collect::<Vec<_>>()
                    .join(", ")
            );
            object([
                (
                    "arguments",
                    Json::Array(
                        arguments
                            .iter()
                            .map(|item| expression_json(item, analyzer, env, false))
                            .collect::<Result<_, _>>()?,
                    ),
                ),
                (
                    "callee",
                    object([
                        ("kind", Json::String("name".into())),
                        ("type", Json::String(signature_type)),
                        ("value", Json::String(name.clone())),
                    ]),
                ),
                ("kind", Json::String("call".into())),
                ("type", Json::String(kind)),
            ])
        }
        Expr::Index { value, index } => object([
            ("index", expression_json(index, analyzer, env, false)?),
            ("kind", Json::String("index".into())),
            ("type", Json::String(kind)),
            ("value", expression_json(value, analyzer, env, false)?),
        ]),
        Expr::Member { value, name } => object([
            ("kind", Json::String("member".into())),
            ("name", Json::String(name.clone())),
            ("type", Json::String(kind)),
            ("value", expression_json(value, analyzer, env, false)?),
        ]),
        Expr::List(items) => object([
            (
                "items",
                Json::Array(
                    items
                        .iter()
                        .map(|item| expression_json(item, analyzer, env, false))
                        .collect::<Result<_, _>>()?,
                ),
            ),
            ("kind", Json::String("list".into())),
            ("type", Json::String(kind)),
        ]),
        Expr::Record(fields) => object([
            (
                "fields",
                Json::Array(
                    fields
                        .iter()
                        .map(|(name, value)| {
                            Ok(object([
                                ("name", Json::String(name.clone())),
                                ("value", expression_json(value, analyzer, env, false)?),
                            ]))
                        })
                        .collect::<Result<_, CompileError>>()?,
                ),
            ),
            ("kind", Json::String("record".into())),
            ("type", Json::String(kind)),
        ]),
    })
}

fn value_json(
    value: &Expression,
    analyzer: &Analyzer,
    env: &HashMap<String, TypeRef>,
    allow_void: bool,
) -> Result<Json, CompileError> {
    expression_json(&value.value, analyzer, env, allow_void)
}

fn statements_json(
    statements: &[Statement],
    analyzer: &Analyzer,
    env: &mut HashMap<String, TypeRef>,
    result: Option<&TypeRef>,
) -> Result<Vec<Json>, CompileError> {
    let mut output = Vec::new();
    for statement in statements {
        let line = statement.line();
        let item = match statement {
            Statement::Let { name, value, .. } => {
                let kind = analyzer.infer(&value.value, env, false).unwrap();
                let json = object([
                    ("kind", Json::String("let".into())),
                    ("line", Json::Number(line.to_string())),
                    ("name", Json::String(name.clone())),
                    ("type", Json::String(kind.canonical())),
                    ("value", value_json(value, analyzer, env, false)?),
                ]);
                env.insert(name.clone(), kind);
                json
            }
            Statement::Show { value, .. }
            | Statement::Fail { value, .. }
            | Statement::Require { value, .. } => {
                let name = match statement {
                    Statement::Show { .. } => "show",
                    Statement::Fail { .. } => "fail",
                    _ => "require",
                };
                object([
                    ("expression", value_json(value, analyzer, env, false)?),
                    ("kind", Json::String(name.into())),
                    ("line", Json::Number(line.to_string())),
                ])
            }
            Statement::Ensure { value, .. } => {
                let mut contract = env.clone();
                if let Some(result) = result {
                    contract.insert("result".into(), result.clone());
                }
                object([
                    ("expression", value_json(value, analyzer, &contract, false)?),
                    ("kind", Json::String("ensure".into())),
                    ("line", Json::Number(line.to_string())),
                ])
            }
            Statement::Return { value, .. } => object([
                ("kind", Json::String("return".into())),
                ("line", Json::Number(line.to_string())),
                (
                    "value",
                    match value {
                        Some(value) => value_json(value, analyzer, env, false)?,
                        None => Json::Null,
                    },
                ),
            ]),
            Statement::If {
                condition,
                then_body,
                else_body,
                ..
            } => object([
                ("condition", value_json(condition, analyzer, env, false)?),
                (
                    "else",
                    Json::Array(statements_json(
                        else_body,
                        analyzer,
                        &mut env.clone(),
                        result,
                    )?),
                ),
                ("kind", Json::String("if".into())),
                ("line", Json::Number(line.to_string())),
                (
                    "then",
                    Json::Array(statements_json(
                        then_body,
                        analyzer,
                        &mut env.clone(),
                        result,
                    )?),
                ),
            ]),
            Statement::Expression { value, .. } => object([
                ("expression", value_json(value, analyzer, env, true)?),
                ("kind", Json::String("expression".into())),
                ("line", Json::Number(line.to_string())),
            ]),
        };
        output.push(item);
    }
    Ok(output)
}

pub fn generate(program: &Program, filename: &str) -> Result<String, CompileError> {
    let analyzer = analyzer(program);
    let records = program
        .records
        .iter()
        .map(|record| {
            object([
                (
                    "fields",
                    Json::Array(
                        record
                            .fields
                            .iter()
                            .map(|field| {
                                object([
                                    ("name", Json::String(field.name.clone())),
                                    ("type", Json::String(field.kind.canonical())),
                                ])
                            })
                            .collect(),
                    ),
                ),
                ("line", Json::Number(record.line.to_string())),
                ("name", Json::String(record.name.clone())),
            ])
        })
        .collect();
    let mut tasks = Vec::new();
    for task in &program.tasks {
        let mut env: HashMap<String, TypeRef> = task
            .parameters
            .iter()
            .map(|parameter| (parameter.name.clone(), parameter.kind.clone()))
            .collect();
        tasks.push(object([
            (
                "body",
                Json::Array(statements_json(
                    &task.body,
                    &analyzer,
                    &mut env,
                    task.result.as_ref(),
                )?),
            ),
            ("line", Json::Number(task.line.to_string())),
            ("name", Json::String(task.name.clone())),
            (
                "parameters",
                Json::Array(
                    task.parameters
                        .iter()
                        .map(|parameter| {
                            object([
                                ("name", Json::String(parameter.name.clone())),
                                ("type", Json::String(parameter.kind.canonical())),
                            ])
                        })
                        .collect(),
                ),
            ),
            (
                "return",
                task.result
                    .as_ref()
                    .map_or(Json::Null, |value| Json::String(value.canonical())),
            ),
        ]));
    }
    let document = object([
        ("aml_ir", Json::String("1.1".into())),
        ("app", Json::String(program.app.clone())),
        ("profile", Json::String(program.profile.clone())),
        ("records", Json::Array(records)),
        ("semantic", Json::String("typed".into())),
        ("source", Json::String(filename.into())),
        ("tasks", Json::Array(tasks)),
    ]);
    Ok(document.pretty(0) + "\n")
}

pub fn compile_source(source: &str, filename: &str) -> Result<String, CompileError> {
    generate(&crate::core::parse(source)?, filename)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emits_typed_ir() {
        let source = "app demo\nprofile script\ntask double(value: Int) -> Int:\n  return value * 2\ntask main:\n  show double(21)\n";
        let generated = compile_source(source, "demo.aml").unwrap();
        assert!(generated.contains("\"aml_ir\": \"1.1\""));
        assert!(generated.contains("\"semantic\": \"typed\""));
        assert!(generated.contains("\"type\": \"Int\""));
        assert!(generated.contains("\"type\": \"Task[Int -> Int]\""));
    }
}
