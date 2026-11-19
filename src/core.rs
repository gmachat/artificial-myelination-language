use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use crate::{
    expression::{self, Expr},
    semantic::{compatible, Analyzer, TaskSignature},
    types::{self, TypeRef},
    CompileError,
};

#[derive(Debug, Clone)]
pub struct Expression {
    pub source: String,
    pub value: Expr,
}

#[derive(Debug, Clone)]
pub enum Statement {
    Let {
        name: String,
        value: Expression,
        line: usize,
    },
    Show {
        value: Expression,
        line: usize,
    },
    Fail {
        value: Expression,
        line: usize,
    },
    Require {
        value: Expression,
        line: usize,
    },
    Ensure {
        value: Expression,
        line: usize,
    },
    Return {
        value: Option<Expression>,
        line: usize,
    },
    If {
        condition: Expression,
        then_body: Vec<Statement>,
        else_body: Vec<Statement>,
        line: usize,
    },
    Expression {
        value: Expression,
        line: usize,
    },
}

impl Statement {
    pub fn line(&self) -> usize {
        match self {
            Self::Let { line, .. }
            | Self::Show { line, .. }
            | Self::Fail { line, .. }
            | Self::Require { line, .. }
            | Self::Ensure { line, .. }
            | Self::Return { line, .. }
            | Self::If { line, .. }
            | Self::Expression { line, .. } => *line,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Field {
    pub name: String,
    pub kind: TypeRef,
}

#[derive(Debug, Clone)]
pub struct Record {
    pub name: String,
    pub fields: Vec<Field>,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub struct Parameter {
    pub name: String,
    pub kind: TypeRef,
}

#[derive(Debug, Clone)]
pub struct Task {
    pub name: String,
    pub parameters: Vec<Parameter>,
    pub result: Option<TypeRef>,
    pub body: Vec<Statement>,
    pub line: usize,
}

#[derive(Debug, Clone)]
pub struct Program {
    pub app: String,
    pub profile: String,
    pub records: Vec<Record>,
    pub tasks: Vec<Task>,
}

#[derive(Debug, Clone)]
struct Line {
    number: usize,
    indent: usize,
    text: String,
}

fn valid_name(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_alphabetic() || first == '_')
        && chars.all(|item| item.is_ascii_alphanumeric() || item == '_')
}

fn source_lines(source: &str) -> Result<Vec<Line>, CompileError> {
    let mut output = Vec::new();
    for (offset, raw) in source.lines().enumerate() {
        let number = offset + 1;
        if raw.contains('\t') {
            return Err(CompileError::new(
                "tabs are not allowed; use two spaces",
                number,
            ));
        }
        if raw.trim().is_empty() || raw.trim_start().starts_with('#') {
            continue;
        }
        let spaces = raw.len() - raw.trim_start_matches(' ').len();
        if spaces % 2 != 0 {
            return Err(CompileError::new(
                "indentation must use multiples of two spaces",
                number,
            ));
        }
        output.push(Line {
            number,
            indent: spaces / 2,
            text: raw.trim().to_string(),
        });
    }
    Ok(output)
}

fn expr(source: &str, line: usize) -> Result<Expression, CompileError> {
    let value = expression::parse(source).map_err(|error| CompileError {
        message: error.message,
        line,
        column: error.column,
    })?;
    Ok(Expression {
        source: source.to_string(),
        value,
    })
}

fn statement(text: &str, line: usize) -> Result<Statement, CompileError> {
    for (keyword, build) in [
        ("show ", 0_u8),
        ("fail ", 1),
        ("require ", 2),
        ("ensure ", 3),
    ] {
        if let Some(source) = text.strip_prefix(keyword) {
            if source.trim().is_empty() {
                return Err(CompileError::new(
                    format!("{} needs an expression", keyword.trim()),
                    line,
                ));
            }
            let value = expr(source.trim(), line)?;
            return Ok(match build {
                0 => Statement::Show { value, line },
                1 => Statement::Fail { value, line },
                2 => Statement::Require { value, line },
                _ => Statement::Ensure { value, line },
            });
        }
    }
    if text == "return" {
        return Ok(Statement::Return { value: None, line });
    }
    if let Some(source) = text.strip_prefix("return ") {
        return Ok(Statement::Return {
            value: Some(expr(source.trim(), line)?),
            line,
        });
    }
    if let Some(source) = text.strip_prefix("let ") {
        let (name, value) = source
            .split_once('=')
            .ok_or_else(|| CompileError::new("use 'let <name> = <expression>'", line))?;
        let name = name.trim();
        if !valid_name(name) || value.trim().is_empty() {
            return Err(CompileError::new("use 'let <name> = <expression>'", line));
        }
        return Ok(Statement::Let {
            name: name.to_string(),
            value: expr(value.trim(), line)?,
            line,
        });
    }
    Ok(Statement::Expression {
        value: expr(text, line)?,
        line,
    })
}

fn body(lines: &[Line], index: &mut usize, indent: usize) -> Result<Vec<Statement>, CompileError> {
    let mut output = Vec::new();
    while *index < lines.len() {
        let line = &lines[*index];
        if line.indent < indent {
            break;
        }
        if line.indent > indent {
            return Err(CompileError::new("unexpected indentation", line.number));
        }
        if let Some(condition) = line
            .text
            .strip_prefix("if ")
            .and_then(|value| value.strip_suffix(':'))
        {
            let number = line.number;
            let condition = expr(condition.trim(), number)?;
            *index += 1;
            let then_body = body(lines, index, indent + 1)?;
            let mut else_body = Vec::new();
            if *index < lines.len()
                && lines[*index].indent == indent
                && lines[*index].text == "else:"
            {
                *index += 1;
                else_body = body(lines, index, indent + 1)?;
            }
            output.push(Statement::If {
                condition,
                then_body,
                else_body,
                line: number,
            });
            continue;
        }
        if line.text == "else:" {
            break;
        }
        output.push(statement(&line.text, line.number)?);
        *index += 1;
    }
    Ok(output)
}

fn parse_task_header(
    text: &str,
    line: usize,
) -> Result<(String, Vec<Parameter>, Option<TypeRef>), CompileError> {
    let raw = text
        .strip_prefix("task ")
        .and_then(|value| value.strip_suffix(':'))
        .ok_or_else(|| CompileError::new("invalid task declaration", line))?;
    let (signature, result) = if let Some((left, right)) = raw.rsplit_once("->") {
        (
            left.trim(),
            Some(types::parse(right.trim()).map_err(|error| CompileError::new(error.0, line))?),
        )
    } else {
        (raw.trim(), None)
    };
    let (name, raw_parameters) = if let Some(open) = signature.find('(') {
        if !signature.ends_with(')') {
            return Err(CompileError::new("invalid task declaration", line));
        }
        (
            &signature[..open],
            &signature[open + 1..signature.len() - 1],
        )
    } else {
        (signature, "")
    };
    if !valid_name(name) {
        return Err(CompileError::new("invalid task declaration", line));
    }
    let mut parameters = Vec::new();
    for raw_parameter in types::split_top_level(raw_parameters) {
        let (parameter_name, parameter_type) = raw_parameter
            .split_once(':')
            .ok_or_else(|| CompileError::new("parameters must use 'name: Type'", line))?;
        if !valid_name(parameter_name.trim()) {
            return Err(CompileError::new("parameters must use 'name: Type'", line));
        }
        parameters.push(Parameter {
            name: parameter_name.trim().to_string(),
            kind: types::parse(parameter_type.trim())
                .map_err(|error| CompileError::new(error.0, line))?,
        });
    }
    Ok((name.to_string(), parameters, result))
}

pub fn parse(source: &str) -> Result<Program, CompileError> {
    let lines = source_lines(source)?;
    if lines.is_empty() {
        return Err(CompileError::new("empty source; expected 'app <name>'", 1));
    }
    let app = lines[0]
        .text
        .strip_prefix("app ")
        .filter(|name| valid_name(name));
    if lines[0].indent != 0 || app.is_none() {
        return Err(CompileError::new(
            "the first declaration must be 'app <name>'",
            lines[0].number,
        ));
    }
    let app = app.unwrap().to_string();
    let mut profile = None;
    let mut records = Vec::new();
    let mut tasks = Vec::new();
    let mut index = 1;
    while index < lines.len() {
        let line = &lines[index];
        if line.indent != 0 {
            return Err(CompileError::new("unexpected indentation", line.number));
        }
        if let Some(value) = line.text.strip_prefix("profile ") {
            if !matches!(value, "script" | "local" | "connected") {
                return Err(CompileError::new(
                    format!("unknown profile '{value}'"),
                    line.number,
                ));
            }
            if profile.replace(value.to_string()).is_some() {
                return Err(CompileError::new(
                    "only one profile is allowed",
                    line.number,
                ));
            }
            index += 1;
            continue;
        }
        if let Some(name) = line
            .text
            .strip_prefix("record ")
            .and_then(|value| value.strip_suffix(':'))
        {
            if !valid_name(name) {
                return Err(CompileError::new("use 'record <Name>:'", line.number));
            }
            let number = line.number;
            index += 1;
            let mut fields = Vec::new();
            while index < lines.len() && lines[index].indent == 1 {
                let field_line = &lines[index];
                let (field_name, field_type) =
                    field_line.text.split_once(':').ok_or_else(|| {
                        CompileError::new("record fields must use 'name: Type'", field_line.number)
                    })?;
                if !valid_name(field_name.trim()) {
                    return Err(CompileError::new(
                        "record fields must use 'name: Type'",
                        field_line.number,
                    ));
                }
                fields.push(Field {
                    name: field_name.trim().to_string(),
                    kind: types::parse(field_type.trim())
                        .map_err(|error| CompileError::new(error.0, field_line.number))?,
                });
                index += 1;
            }
            records.push(Record {
                name: name.to_string(),
                fields,
                line: number,
            });
            continue;
        }
        if line.text.starts_with("task ") {
            let number = line.number;
            let (name, parameters, result) = parse_task_header(&line.text, number)?;
            index += 1;
            let statements = body(&lines, &mut index, 1)?;
            tasks.push(Task {
                name,
                parameters,
                result,
                body: statements,
                line: number,
            });
            continue;
        }
        return Err(CompileError::new(
            "only profile, record, and task declarations are allowed",
            line.number,
        ));
    }
    let profile = profile
        .ok_or_else(|| CompileError::new("application must declare a profile", lines[0].number))?;
    if tasks.is_empty() {
        return Err(CompileError::new(
            "application must define at least one task",
            lines[0].number,
        ));
    }
    let program = Program {
        app,
        profile,
        records,
        tasks,
    };
    validate(&program)?;
    Ok(program)
}

fn validate_statements(
    statements: &[Statement],
    analyzer: &Analyzer,
    env: &mut HashMap<String, TypeRef>,
    task: &Task,
    nested: bool,
) -> Result<bool, CompileError> {
    let mut returned = false;
    for statement in statements {
        let line = statement.line();
        match statement {
            Statement::Let { name, value, .. } => {
                let actual = analyzer
                    .infer(&value.value, env, false)
                    .map_err(|message| CompileError::new(message, line))?;
                env.insert(name.clone(), actual);
            }
            Statement::Show { value, .. } => {
                analyzer
                    .infer(&value.value, env, false)
                    .map_err(|message| CompileError::new(message, line))?;
            }
            Statement::Fail { value, .. } => {
                let actual = analyzer
                    .infer(&value.value, env, false)
                    .map_err(|message| CompileError::new(message, line))?;
                if actual.name != "Text" {
                    return Err(CompileError::new(
                        format!("fail expects Text, got {}", actual.canonical()),
                        line,
                    ));
                }
            }
            Statement::Require { value, .. } => {
                let actual = analyzer
                    .infer(&value.value, env, false)
                    .map_err(|message| CompileError::new(message, line))?;
                if actual.name != "Bool" {
                    return Err(CompileError::new(
                        format!("require expects Bool, got {}", actual.canonical()),
                        line,
                    ));
                }
            }
            Statement::Ensure { value, .. } => {
                if nested {
                    return Err(CompileError::new(
                        "ensure must be at the top level of a task",
                        line,
                    ));
                }
                let Some(expected) = &task.result else {
                    return Err(CompileError::new(
                        "ensure requires a declared return type",
                        line,
                    ));
                };
                let mut contract = env.clone();
                contract.insert("result".into(), expected.clone());
                let actual = analyzer
                    .infer(&value.value, &contract, false)
                    .map_err(|message| CompileError::new(message, line))?;
                if actual.name != "Bool" {
                    return Err(CompileError::new(
                        format!("ensure expects Bool, got {}", actual.canonical()),
                        line,
                    ));
                }
            }
            Statement::Return { value, .. } => {
                returned = true;
                match (value, &task.result) {
                    (None, Some(expected)) => {
                        return Err(CompileError::new(
                            format!("task '{}' must return {}", task.name, expected.canonical()),
                            line,
                        ))
                    }
                    (Some(value), Some(expected)) => {
                        let actual = analyzer
                            .infer(&value.value, env, false)
                            .map_err(|message| CompileError::new(message, line))?;
                        if !compatible(&actual, expected) {
                            return Err(CompileError::new(
                                format!(
                                    "task '{}' returns {}, expected {}",
                                    task.name,
                                    actual.canonical(),
                                    expected.canonical()
                                ),
                                line,
                            ));
                        }
                    }
                    (Some(value), None) => {
                        analyzer
                            .infer(&value.value, env, false)
                            .map_err(|message| CompileError::new(message, line))?;
                    }
                    _ => {}
                }
            }
            Statement::If {
                condition,
                then_body,
                else_body,
                ..
            } => {
                let actual = analyzer
                    .infer(&condition.value, env, false)
                    .map_err(|message| CompileError::new(message, line))?;
                if actual.name != "Bool" {
                    return Err(CompileError::new(
                        format!("if expects Bool, got {}", actual.canonical()),
                        line,
                    ));
                }
                let then_return =
                    validate_statements(then_body, analyzer, &mut env.clone(), task, true)?;
                let else_return =
                    validate_statements(else_body, analyzer, &mut env.clone(), task, true)?;
                returned |= then_return || else_return;
            }
            Statement::Expression { value, .. } => {
                analyzer
                    .infer(&value.value, env, true)
                    .map_err(|message| CompileError::new(message, line))?;
            }
        }
    }
    Ok(returned)
}

pub fn validate(program: &Program) -> Result<(), CompileError> {
    let mut record_names = HashSet::new();
    for record in &program.records {
        if !record_names.insert(record.name.clone()) {
            return Err(CompileError::new(
                format!("duplicate record '{}'", record.name),
                record.line,
            ));
        }
    }
    let known: BTreeSet<String> = record_names.iter().cloned().collect();
    let mut analyzer = Analyzer::default();
    for record in &program.records {
        if record.fields.is_empty() {
            return Err(CompileError::new(
                format!("record '{}' needs at least one field", record.name),
                record.line,
            ));
        }
        let mut fields = BTreeMap::new();
        for field in &record.fields {
            types::validate(&field.kind, &known)
                .map_err(|error| CompileError::new(error.0, record.line))?;
            if fields
                .insert(field.name.clone(), field.kind.clone())
                .is_some()
            {
                return Err(CompileError::new(
                    format!("duplicate field '{}'", field.name),
                    record.line,
                ));
            }
        }
        analyzer.records.insert(record.name.clone(), fields);
    }
    let mut task_names = HashSet::new();
    for task in &program.tasks {
        if !task_names.insert(task.name.clone()) {
            return Err(CompileError::new(
                format!("duplicate task '{}'", task.name),
                task.line,
            ));
        }
        let mut names = HashSet::new();
        for parameter in &task.parameters {
            if !names.insert(parameter.name.clone()) {
                return Err(CompileError::new(
                    format!("duplicate parameter '{}'", parameter.name),
                    task.line,
                ));
            }
            types::validate(&parameter.kind, &known)
                .map_err(|error| CompileError::new(error.0, task.line))?;
        }
        if let Some(result) = &task.result {
            types::validate(result, &known)
                .map_err(|error| CompileError::new(error.0, task.line))?;
        }
        if task.name == "main" && (!task.parameters.is_empty() || task.result.is_some()) {
            return Err(CompileError::new(
                "main cannot have parameters or a return type",
                task.line,
            ));
        }
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
    for task in &program.tasks {
        let mut env: HashMap<String, TypeRef> = task
            .parameters
            .iter()
            .map(|item| (item.name.clone(), item.kind.clone()))
            .collect();
        let returned = validate_statements(&task.body, &analyzer, &mut env, task, false)?;
        if task.result.is_some() && !returned {
            return Err(CompileError::new(
                format!(
                    "task '{}' declares a return type but has no return",
                    task.name
                ),
                task.line,
            ));
        }
        if task
            .body
            .iter()
            .any(|item| matches!(item, Statement::Ensure { .. }))
            && !returned
        {
            return Err(CompileError::new(
                format!("task '{}' has ensure contracts but no return", task.name),
                task.line,
            ));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_and_validates_core_program() {
        let source = "app scores\nprofile script\nrecord Person:\n  name: Text\ntask greet(person: Person) -> Text:\n  ensure result != \"\"\n  return \"Hello, \" + person.name\ntask main:\n  show greet({name: \"Ada\"})\n";
        let program = parse(source).unwrap();
        assert_eq!(program.records[0].name, "Person");
        assert_eq!(program.tasks.len(), 2);
    }
    #[test]
    fn rejects_wrong_return_type() {
        let source = "app bad\nprofile script\ntask wrong() -> Int:\n  return \"no\"\n";
        assert!(parse(source)
            .unwrap_err()
            .message
            .contains("returns Text, expected Int"));
    }
}
