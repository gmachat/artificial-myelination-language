use std::collections::HashMap;

use crate::{
    expression::{parse as parse_expression, render_javascript},
    semantic::{compatible, Analyzer},
    types::{parse as parse_type, TypeRef},
    CompileError,
};

const NAME_HELP: &str =
    "names must begin with a letter or underscore and contain letters, digits, or underscores";

const RUNTIME: &str = r#"function __amlActual(value) {
  if (value === null) return "none";
  if (Array.isArray(value)) return "list";
  return typeof value;
}

function __amlExpect(place, value, expected) {
  const valid =
    expected === "Any" ||
    (expected === "Bool" && typeof value === "boolean") ||
    (expected === "Float" && typeof value === "number" && Number.isFinite(value)) ||
    (expected === "Int" && Number.isInteger(value)) ||
    (expected === "Text" && typeof value === "string");
  if (!valid) {
    throw new Error(`AML type | ${place} expected ${expected}, got ${__amlActual(value)}`);
  }
  return value;
}

function __amlRequire(condition, component, expression) {
  if (!condition) {
    throw new Error(`AML contract | ${component} require failed: ${expression}`);
  }
}"#;

#[derive(Debug, Clone)]
struct Line {
    number: usize,
    indent: usize,
    text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Kind {
    Column,
    Row,
    When,
    Heading,
    Text,
    Input,
    Button,
    Click,
    Set,
    Show,
    Name,
}

#[derive(Debug, Clone)]
struct Node {
    kind: Kind,
    value: Option<String>,
    line: usize,
    children: Vec<Node>,
    alternate: Vec<Node>,
}

impl Node {
    fn leaf(kind: Kind, value: impl Into<Option<String>>, line: usize) -> Self {
        Self {
            kind,
            value: value.into(),
            line,
            children: Vec::new(),
            alternate: Vec::new(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ComponentInfo {
    pub app_name: String,
    pub component_name: String,
    pub props: Vec<(String, String)>,
}

fn valid_name(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_alphabetic() || first == '_')
        && chars.all(|item| item.is_ascii_alphanumeric() || item == '_')
}

fn valid_type(value: &str) -> bool {
    matches!(value, "Any" | "Bool" | "Float" | "Int" | "Text")
}

fn lines(source: &str) -> Result<Vec<Line>, CompileError> {
    let mut result = Vec::new();
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
        let leading = raw.len() - raw.trim_start_matches(' ').len();
        if leading % 2 != 0 {
            return Err(CompileError::new(
                "indentation must use multiples of two spaces",
                number,
            ));
        }
        result.push(Line {
            number,
            indent: leading / 2,
            text: raw.trim().to_string(),
        });
    }
    Ok(result)
}

fn split_declaration<'a>(text: &'a str, prefix: &str, suffix: &str) -> Option<&'a str> {
    text.strip_prefix(prefix)?.strip_suffix(suffix)
}

fn parse_children(
    items: &[Line],
    index: &mut usize,
    parent_indent: usize,
) -> Result<Vec<Node>, CompileError> {
    let mut nodes = Vec::new();
    let expected = parent_indent + 1;
    while *index < items.len() {
        let line = &items[*index];
        if line.indent <= parent_indent {
            break;
        }
        if line.indent != expected {
            return Err(CompileError::new(
                "unexpected view indentation",
                line.number,
            ));
        }
        let text = line.text.as_str();
        if matches!(text, "column:" | "row:") {
            let kind = if text == "column:" {
                Kind::Column
            } else {
                Kind::Row
            };
            let label = text.trim_end_matches(':');
            let number = line.number;
            *index += 1;
            let children = parse_children(items, index, line.indent)?;
            if children.is_empty() {
                return Err(CompileError::new(
                    format!("{label} requires at least one child"),
                    number,
                ));
            }
            nodes.push(Node {
                kind,
                value: None,
                line: number,
                children,
                alternate: Vec::new(),
            });
            continue;
        }
        if let Some(condition) = split_declaration(text, "when ", ":") {
            if condition.trim().is_empty() {
                return Err(CompileError::new("when needs a condition", line.number));
            }
            let number = line.number;
            let condition = condition.trim().to_string();
            *index += 1;
            let children = parse_children(items, index, line.indent)?;
            if children.is_empty() {
                return Err(CompileError::new(
                    "when requires at least one child",
                    number,
                ));
            }
            nodes.push(Node {
                kind: Kind::When,
                value: Some(condition),
                line: number,
                children,
                alternate: Vec::new(),
            });
            continue;
        }
        if text == "else:" {
            let number = line.number;
            *index += 1;
            let alternate = parse_children(items, index, line.indent)?;
            if alternate.is_empty() {
                return Err(CompileError::new(
                    "else requires at least one child",
                    number,
                ));
            }
            let previous = nodes.last_mut().ok_or_else(|| {
                CompileError::new("else must immediately follow a when block", number)
            })?;
            if previous.kind != Kind::When || !previous.alternate.is_empty() {
                return Err(CompileError::new(
                    "else must immediately follow a when block",
                    number,
                ));
            }
            previous.alternate = alternate;
            continue;
        }
        if let Some(value) = text.strip_prefix("heading ") {
            nodes.push(Node::leaf(
                Kind::Heading,
                Some(value.trim().to_string()),
                line.number,
            ));
            *index += 1;
            continue;
        }
        if let Some(value) = text.strip_prefix("text ") {
            nodes.push(Node::leaf(
                Kind::Text,
                Some(value.trim().to_string()),
                line.number,
            ));
            *index += 1;
            continue;
        }
        if let Some(value) = text.strip_prefix("input ") {
            let name = value.trim();
            if !valid_name(name) {
                return Err(CompileError::new("input expects a state name", line.number));
            }
            nodes.push(Node::leaf(Kind::Input, Some(name.to_string()), line.number));
            *index += 1;
            continue;
        }
        if let Some(label) = split_declaration(text, "button ", ":") {
            let number = line.number;
            let label = label.trim().to_string();
            *index += 1;
            let children = parse_children(items, index, line.indent)?;
            if children.len() != 1 || children[0].kind != Kind::Click {
                return Err(CompileError::new(
                    "button requires one 'on click' block",
                    number,
                ));
            }
            nodes.push(Node {
                kind: Kind::Button,
                value: Some(label),
                line: number,
                children,
                alternate: Vec::new(),
            });
            continue;
        }
        if text == "on click:" {
            let number = line.number;
            *index += 1;
            let children = parse_children(items, index, line.indent)?;
            if children.is_empty() {
                return Err(CompileError::new("on click requires an action", number));
            }
            nodes.push(Node {
                kind: Kind::Click,
                value: None,
                line: number,
                children,
                alternate: Vec::new(),
            });
            continue;
        }
        if let Some(rest) = text.strip_prefix("set ") {
            let (name, value) = rest.split_once('=').ok_or_else(|| {
                CompileError::new("use 'set <state> = <expression>'", line.number)
            })?;
            let name = name.trim();
            if !valid_name(name) || value.trim().is_empty() {
                return Err(CompileError::new(
                    "use 'set <state> = <expression>'",
                    line.number,
                ));
            }
            let name_node = Node::leaf(Kind::Name, Some(name.to_string()), line.number);
            nodes.push(Node {
                kind: Kind::Set,
                value: Some(value.trim().to_string()),
                line: line.number,
                children: vec![name_node],
                alternate: Vec::new(),
            });
            *index += 1;
            continue;
        }
        if let Some(value) = text.strip_prefix("show ") {
            nodes.push(Node::leaf(
                Kind::Show,
                Some(value.trim().to_string()),
                line.number,
            ));
            *index += 1;
            continue;
        }
        return Err(CompileError::new(
            format!("unknown React view construct: {text}"),
            line.number,
        ));
    }
    Ok(nodes)
}

fn js_string(value: &str) -> String {
    let escaped = value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n");
    format!("\"{escaped}\"")
}

fn expression(source: &str) -> Result<String, CompileError> {
    render_javascript(source).map_err(|error| CompileError {
        message: error.message,
        line: 1,
        column: error.column,
    })
}

fn view_expression(source: &str) -> Result<String, CompileError> {
    if source.starts_with('"') && source.ends_with('"') && source.len() >= 2 {
        let content = &source[1..source.len() - 1];
        let mut rendered = String::new();
        let mut rest = content;
        let mut found = false;
        while let Some(open) = rest.find('{') {
            let after = &rest[open + 1..];
            let Some(close) = after.find('}') else {
                break;
            };
            let name = &after[..close];
            if !valid_name(name) {
                break;
            }
            found = true;
            rendered.push_str(&rest[..open].replace('`', "\\`"));
            rendered.push_str("${");
            rendered.push_str(name);
            rendered.push('}');
            rest = &after[close + 1..];
        }
        if found {
            rendered.push_str(&rest.replace('`', "\\`"));
            return Ok(format!("`{rendered}`"));
        }
    }
    expression(source)
}

fn setter(name: &str) -> String {
    let mut result = String::from("set");
    for part in name.split('_') {
        let mut chars = part.chars();
        if let Some(first) = chars.next() {
            result.extend(first.to_uppercase());
            result.extend(chars);
        }
    }
    result
}

fn render_node(
    node: &Node,
    indent: usize,
    states: &HashMap<String, String>,
) -> Result<Vec<String>, CompileError> {
    let pad = "  ".repeat(indent);
    match node.kind {
        Kind::Column | Kind::Row => {
            let direction = if node.kind == Kind::Column {
                "column"
            } else {
                "row"
            };
            let mut output = vec![format!(
                r#"{pad}<div style={{{{ display: "flex", flexDirection: "{direction}", gap: "0.75rem" }}}}>"#
            )];
            for child in &node.children {
                output.extend(render_node(child, indent + 1, states)?);
            }
            output.push(format!("{pad}</div>"));
            Ok(output)
        }
        Kind::When => {
            let condition = expression(node.value.as_deref().unwrap_or_default())?;
            let operator = if node.alternate.is_empty() { "&&" } else { "?" };
            let mut output = vec![
                format!("{pad}{{Boolean({condition}) {operator} ("),
                format!("{pad}  <>"),
            ];
            for child in &node.children {
                output.extend(render_node(child, indent + 2, states)?);
            }
            output.extend([format!("{pad}  </>"), format!("{pad})")]);
            if !node.alternate.is_empty() {
                output.last_mut().unwrap().push_str(" : (");
                output.push(format!("{pad}  <>"));
                for child in &node.alternate {
                    output.extend(render_node(child, indent + 2, states)?);
                }
                output.extend([format!("{pad}  </>"), format!("{pad})")]);
            }
            output.last_mut().unwrap().push('}');
            Ok(output)
        }
        Kind::Heading => Ok(vec![format!(
            "{pad}<h1>{{{}}}</h1>",
            view_expression(node.value.as_deref().unwrap_or_default())?
        )]),
        Kind::Text => Ok(vec![format!(
            "{pad}<p>{{{}}}</p>",
            view_expression(node.value.as_deref().unwrap_or_default())?
        )]),
        Kind::Input => {
            let name = node.value.as_deref().unwrap_or_default();
            let Some(kind) = states.get(name) else {
                return Err(CompileError::new(
                    format!("input references unknown state '{name}'"),
                    node.line,
                ));
            };
            if kind != "Text" {
                return Err(CompileError::new(
                    "input state must have type Text",
                    node.line,
                ));
            }
            Ok(vec![format!(
                r#"{pad}<input name="{name}" aria-label="{name}" value={{{name}}} onChange={{event => {}(event.target.value)}} />"#,
                setter(name)
            )])
        }
        Kind::Button => {
            let mut actions = Vec::new();
            for action in &node.children[0].children {
                match action.kind {
                    Kind::Set => {
                        let name = action.children[0].value.as_deref().unwrap_or_default();
                        if !states.contains_key(name) {
                            return Err(CompileError::new(
                                format!("set references unknown state '{name}'"),
                                action.line,
                            ));
                        }
                        actions.push(format!(
                            "{}({});",
                            setter(name),
                            expression(action.value.as_deref().unwrap_or_default())?
                        ));
                    }
                    Kind::Show => actions.push(format!(
                        "console.log({});",
                        expression(action.value.as_deref().unwrap_or_default())?
                    )),
                    _ => return Err(CompileError::new("unsupported click action", action.line)),
                }
            }
            let label = view_expression(node.value.as_deref().unwrap_or("\"\""))?;
            Ok(vec![format!(
                r#"{pad}<button type="button" onClick={{() => {{ {} }}}}>{{{label}}}</button>"#,
                actions.join(" ")
            )])
        }
        _ => Err(CompileError::new(
            "cannot render React construct",
            node.line,
        )),
    }
}

fn validate_node(
    node: &Node,
    analyzer: &Analyzer,
    names: &HashMap<String, TypeRef>,
) -> Result<(), CompileError> {
    let infer = |source: &str| {
        let expression = parse_expression(source).map_err(|error| CompileError {
            message: error.message,
            line: node.line,
            column: error.column,
        })?;
        analyzer
            .infer(&expression, names, false)
            .map_err(|message| CompileError::new(message, node.line))
    };
    match node.kind {
        Kind::When => {
            let actual = infer(node.value.as_deref().unwrap_or_default())?;
            if actual.name != "Bool" {
                return Err(CompileError::new(
                    format!("when expects Bool, got {}", actual.canonical()),
                    node.line,
                ));
            }
        }
        Kind::Heading | Kind::Text | Kind::Button => {
            infer(node.value.as_deref().unwrap_or("\"\""))?;
        }
        Kind::Set => {
            let name = node.children[0].value.as_deref().unwrap_or_default();
            let expected = names.get(name).ok_or_else(|| {
                CompileError::new(format!("set references unknown state '{name}'"), node.line)
            })?;
            let actual = infer(node.value.as_deref().unwrap_or_default())?;
            if !compatible(&actual, expected) {
                return Err(CompileError::new(
                    format!(
                        "state '{name}' expects {}, got {}",
                        expected.canonical(),
                        actual.canonical()
                    ),
                    node.line,
                ));
            }
        }
        Kind::Show => {
            infer(node.value.as_deref().unwrap_or_default())?;
        }
        _ => {}
    }
    for child in &node.children {
        validate_node(child, analyzer, names)?;
    }
    for child in &node.alternate {
        validate_node(child, analyzer, names)?;
    }
    Ok(())
}

type ParsedComponent = (
    ComponentInfo,
    Vec<(String, String, String)>,
    Vec<String>,
    Node,
);

fn parse_component(source: &str) -> Result<ParsedComponent, CompileError> {
    let items = lines(source)?;
    if items.is_empty() {
        return Err(CompileError::new("empty source; expected 'app <name>'", 1));
    }
    let app = items[0]
        .text
        .strip_prefix("app ")
        .filter(|value| valid_name(value));
    if items[0].indent != 0 || app.is_none() {
        return Err(CompileError::new(
            "the first declaration must be 'app <name>'",
            items[0].number,
        ));
    }
    let app_name = app.unwrap().to_string();
    if items.len() < 2
        || items[1].indent != 0
        || !matches!(items[1].text.as_str(), "profile react" | "profile web")
    {
        return Err(CompileError::new(
            "React target requires 'profile react' or 'profile web'",
            items.get(1).map_or(items[0].number, |line| line.number),
        ));
    }
    let declaration = items.get(2).ok_or_else(|| {
        CompileError::new("React application must define a component", items[1].number)
    })?;
    let body = split_declaration(&declaration.text, "component ", ":").ok_or_else(|| {
        CompileError::new("expected 'component <Name>(props):'", declaration.number)
    })?;
    if declaration.indent != 0 {
        return Err(CompileError::new(
            "expected 'component <Name>(props):'",
            declaration.number,
        ));
    }
    let (component_name, raw_props) = if let Some(open) = body.find('(') {
        if !body.ends_with(')') {
            return Err(CompileError::new(
                "expected 'component <Name>(props):'",
                declaration.number,
            ));
        }
        (&body[..open], Some(&body[open + 1..body.len() - 1]))
    } else {
        (body, None)
    };
    if !valid_name(component_name) {
        return Err(CompileError::new(NAME_HELP, declaration.number));
    }
    if !component_name.chars().next().unwrap().is_ascii_uppercase() {
        return Err(CompileError::new(
            "component names must begin with an uppercase letter",
            declaration.number,
        ));
    }
    let mut props = Vec::new();
    if let Some(raw) = raw_props {
        for item in raw.split(',') {
            let (name, kind) = item.split_once(':').ok_or_else(|| {
                CompileError::new("props must use 'name: Type'", declaration.number)
            })?;
            let (name, kind) = (name.trim(), kind.trim());
            if !valid_name(name) || !valid_type(kind) {
                return Err(CompileError::new(
                    "props must use 'name: Type'",
                    declaration.number,
                ));
            }
            props.push((name.to_string(), kind.to_string()));
        }
    }
    let mut index = 3;
    let mut states = Vec::new();
    let mut state_names = HashMap::new();
    let mut requirements = Vec::new();
    let mut view: Option<Vec<Node>> = None;
    while index < items.len() {
        let line = &items[index];
        if line.indent != 1 {
            return Err(CompileError::new(
                "unexpected component indentation",
                line.number,
            ));
        }
        if let Some(rest) = line.text.strip_prefix("state ") {
            let (left, value) = rest.split_once('=').ok_or_else(|| {
                CompileError::new("state must use 'name: Type = value'", line.number)
            })?;
            let (name, kind) = left.split_once(':').ok_or_else(|| {
                CompileError::new("state must use 'name: Type = value'", line.number)
            })?;
            let (name, kind, value) = (name.trim(), kind.trim(), value.trim());
            if !valid_name(name) || !valid_type(kind) || value.is_empty() {
                return Err(CompileError::new(
                    "state must use 'name: Type = value'",
                    line.number,
                ));
            }
            if state_names.contains_key(name) {
                return Err(CompileError::new(
                    format!("duplicate state '{name}'"),
                    line.number,
                ));
            }
            state_names.insert(name.to_string(), kind.to_string());
            states.push((name.to_string(), kind.to_string(), value.to_string()));
            index += 1;
            continue;
        }
        if let Some(condition) = line.text.strip_prefix("require ") {
            if condition.trim().is_empty() {
                return Err(CompileError::new("require needs a condition", line.number));
            }
            requirements.push(condition.trim().to_string());
            index += 1;
            continue;
        }
        if line.text == "view:" {
            if view.is_some() {
                return Err(CompileError::new(
                    "component may only have one view",
                    line.number,
                ));
            }
            index += 1;
            let children = parse_children(&items, &mut index, line.indent)?;
            if children.is_empty() {
                return Err(CompileError::new(
                    "view requires at least one element",
                    line.number,
                ));
            }
            view = Some(children);
            continue;
        }
        return Err(CompileError::new(
            format!("unknown component construct: {}", line.text),
            line.number,
        ));
    }
    let view =
        view.ok_or_else(|| CompileError::new("component must define a view", declaration.number))?;
    if view.len() != 1 {
        return Err(CompileError::new(
            "view must contain one root element",
            declaration.number,
        ));
    }
    Ok((
        ComponentInfo {
            app_name,
            component_name: component_name.to_string(),
            props,
        },
        states,
        requirements,
        view.into_iter().next().unwrap(),
    ))
}

pub fn component_info(source: &str) -> Result<ComponentInfo, CompileError> {
    parse_component(source).map(|result| result.0)
}

pub fn compile(source: &str) -> Result<String, CompileError> {
    let (info, states, requirements, view) = parse_component(source)?;
    let analyzer = Analyzer::default();
    let mut names: HashMap<String, TypeRef> = HashMap::new();
    for (name, kind) in &info.props {
        names.insert(name.clone(), parse_type(kind).unwrap());
    }
    for (name, kind, value) in &states {
        let expected = parse_type(kind).unwrap();
        let parsed = parse_expression(value).map_err(|error| CompileError {
            message: error.message,
            line: 1,
            column: error.column,
        })?;
        let actual = analyzer
            .infer(&parsed, &names, false)
            .map_err(|message| CompileError::new(message, 1))?;
        if !compatible(&actual, &expected) {
            return Err(CompileError::new(
                format!(
                    "state '{name}' expects {}, got {}",
                    expected.canonical(),
                    actual.canonical()
                ),
                1,
            ));
        }
        names.insert(name.clone(), expected);
    }
    for condition in &requirements {
        let parsed = parse_expression(condition).map_err(|error| CompileError {
            message: error.message,
            line: 1,
            column: error.column,
        })?;
        let actual = analyzer
            .infer(&parsed, &names, false)
            .map_err(|message| CompileError::new(message, 1))?;
        if actual.name != "Bool" {
            return Err(CompileError::new(
                format!("require expects Bool, got {}", actual.canonical()),
                1,
            ));
        }
    }
    validate_node(&view, &analyzer, &names)?;
    let props_map = info
        .props
        .iter()
        .map(|(name, _)| name.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let parameter = if props_map.is_empty() {
        String::new()
    } else {
        format!("{{ {props_map} }}")
    };
    let mut output = vec![
        format!(
            "// Generated by AML {VERSION}. Do not edit generated code.",
            VERSION = crate::VERSION
        ),
        "import React, { useState } from \"react\";".to_string(),
        String::new(),
        RUNTIME.to_string(),
        String::new(),
        format!("export function {}({parameter}) {{", info.component_name),
    ];
    for (name, kind) in &info.props {
        output.push(format!(
            "  __amlExpect({}, {name}, {});",
            js_string(&format!("{}.{}", info.component_name, name)),
            js_string(kind)
        ));
    }
    let state_map: HashMap<String, String> = states
        .iter()
        .map(|(name, kind, _)| (name.clone(), kind.clone()))
        .collect();
    for (name, kind, value) in &states {
        output.push(format!(
            "  const [{name}, {}] = useState(() => __amlExpect({}, {}, {}));",
            setter(name),
            js_string(&format!("{}.{}", info.component_name, name)),
            expression(value)?,
            js_string(kind)
        ));
        output.push(format!(
            "  __amlExpect({}, {name}, {});",
            js_string(&format!("{}.{}", info.component_name, name)),
            js_string(kind)
        ));
    }
    for condition in requirements {
        output.push(format!(
            "  __amlRequire(Boolean({}), {}, {});",
            expression(&condition)?,
            js_string(&info.component_name),
            js_string(&condition)
        ));
    }
    output.extend([String::new(), "  return (".to_string()]);
    output.extend(render_node(&view, 2, &state_map)?);
    output.extend([
        "  );".to_string(),
        "}".to_string(),
        String::new(),
        format!("export default {};", info.component_name),
        String::new(),
    ]);
    Ok(output.join("\n"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const COUNTER: &str = r#"app counter
profile web
component Counter(title: Text):
  state count: Int = 0
  view:
    column:
      heading title
      when count == 0:
        text "Ready"
      else:
        text "Started"
      button "Add":
        on click:
          set count = count + 1
"#;

    #[test]
    fn compiles_state_events_and_conditionals() {
        let generated = compile(COUNTER).unwrap();
        assert!(generated.contains("setCount(count + 1);"));
        assert!(generated.contains("Boolean(count === 0) ? ("));
        assert!(generated.contains("export default Counter;"));
    }

    #[test]
    fn rejects_orphan_else() {
        let bad = "app bad\nprofile web\ncomponent Bad:\n  view:\n    column:\n      else:\n        text \"no\"\n";
        assert!(compile(bad)
            .unwrap_err()
            .message
            .contains("else must immediately follow"));
    }

    #[test]
    fn rejects_wrong_state_update_type() {
        let bad = "app bad\nprofile web\ncomponent Bad:\n  state count: Int = 0\n  view:\n    button \"bad\":\n      on click:\n        set count = \"no\"\n";
        assert!(compile(bad)
            .unwrap_err()
            .message
            .contains("state 'count' expects Int, got Text"));
    }

    #[test]
    fn rejects_non_boolean_when() {
        let bad = "app bad\nprofile web\ncomponent Bad:\n  state count: Int = 0\n  view:\n    when count:\n      text \"no\"\n";
        assert!(compile(bad)
            .unwrap_err()
            .message
            .contains("when expects Bool, got Int"));
    }
}
