use std::{
    collections::BTreeMap,
    io::{self, BufRead, Write},
};

use crate::{
    core, format_source, fullstack,
    json::{self, Value},
    react, CompileError, VERSION,
};

fn object(values: impl IntoIterator<Item = (impl Into<String>, Value)>) -> Value {
    Value::Object(
        values
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
    )
}

fn get<'a>(value: &'a Value, path: &[&str]) -> Option<&'a Value> {
    let mut current = value;
    for key in path {
        current = current.object().ok()?.get(*key)?;
    }
    Some(current)
}

fn string<'a>(value: &'a Value, path: &[&str]) -> Option<&'a str> {
    get(value, path)?.string().ok()
}

pub fn read_message<R: BufRead>(input: &mut R) -> Result<Option<Value>, String> {
    let mut length = None;
    loop {
        let mut line = String::new();
        if input
            .read_line(&mut line)
            .map_err(|error| error.to_string())?
            == 0
        {
            return Ok(None);
        }
        if line == "\r\n" || line == "\n" {
            break;
        }
        if let Some((name, value)) = line.split_once(':') {
            if name.eq_ignore_ascii_case("content-length") {
                length = Some(
                    value
                        .trim()
                        .parse::<usize>()
                        .map_err(|_| "invalid LSP content length")?,
                );
            }
        }
    }
    let length = length.ok_or("LSP message is missing Content-Length")?;
    let mut bytes = vec![0; length];
    input
        .read_exact(&mut bytes)
        .map_err(|error| error.to_string())?;
    json::parse(std::str::from_utf8(&bytes).map_err(|error| error.to_string())?).map(Some)
}

pub fn write_message<W: Write>(output: &mut W, value: &Value) -> Result<(), String> {
    let body = json::pretty(value);
    write!(output, "Content-Length: {}\r\n\r\n{}", body.len(), body)
        .map_err(|error| error.to_string())?;
    output.flush().map_err(|error| error.to_string())
}

fn error_for(source: &str) -> Option<CompileError> {
    let profile = source
        .lines()
        .map(str::trim)
        .find_map(|line| line.strip_prefix("profile "))
        .unwrap_or("script");
    match profile {
        "fullstack" => fullstack::parse(source).err(),
        "react" | "web" => react::compile(source).err(),
        _ => core::parse(source).err(),
    }
}

fn diagnostics(source: &str) -> Value {
    let values = error_for(source).map_or_else(Vec::new, |error| {
        let line = error.line.saturating_sub(1);
        let character = error.column.saturating_sub(1);
        vec![object([
            ("code", Value::String("AML1000".into())),
            ("message", Value::String(error.message)),
            (
                "range",
                object([
                    (
                        "end",
                        object([
                            ("character", Value::Number((character + 1).to_string())),
                            ("line", Value::Number(line.to_string())),
                        ]),
                    ),
                    (
                        "start",
                        object([
                            ("character", Value::Number(character.to_string())),
                            ("line", Value::Number(line.to_string())),
                        ]),
                    ),
                ]),
            ),
            ("severity", Value::Number("1".into())),
            ("source", Value::String("aml".into())),
        ])]
    });
    Value::Array(values)
}

fn response(id: Value, result: Value) -> Value {
    object([
        ("id", id),
        ("jsonrpc", Value::String("2.0".into())),
        ("result", result),
    ])
}
fn notification(method: &str, params: Value) -> Value {
    object([
        ("jsonrpc", Value::String("2.0".into())),
        ("method", Value::String(method.into())),
        ("params", params),
    ])
}

fn completion_items() -> Value {
    let keywords = [
        "app",
        "module",
        "use",
        "profile",
        "task",
        "let",
        "show",
        "require",
        "ensure",
        "return",
        "fail",
        "if",
        "when",
        "else",
        "record",
        "data",
        "resource",
        "page",
        "storage",
        "component",
        "state",
        "view",
    ];
    let types = [
        "Any",
        "Bool",
        "Float",
        "Int",
        "Text",
        "List[T]",
        "Optional[T]",
        "Time",
    ];
    Value::Array(
        keywords
            .into_iter()
            .map(|label| {
                object([
                    ("detail", Value::String("AML keyword".into())),
                    ("kind", Value::Number("14".into())),
                    ("label", Value::String(label.into())),
                ])
            })
            .chain(types.into_iter().map(|label| {
                object([
                    ("detail", Value::String("AML type".into())),
                    ("kind", Value::Number("7".into())),
                    ("label", Value::String(label.into())),
                ])
            }))
            .collect(),
    )
}

pub fn serve<R: BufRead, W: Write>(input: &mut R, output: &mut W) -> Result<(), String> {
    let mut documents: BTreeMap<String, String> = BTreeMap::new();
    while let Some(message) = read_message(input)? {
        let method = string(&message, &["method"]).unwrap_or("");
        let id = get(&message, &["id"]).cloned();
        match method {
            "initialize" => {
                if let Some(id) = id {
                    write_message(
                        output,
                        &response(
                            id,
                            object([
                                (
                                    "capabilities",
                                    object([
                                        (
                                            "completionProvider",
                                            object([(
                                                "triggerCharacters",
                                                Value::Array(vec![
                                                    Value::String(" ".into()),
                                                    Value::String(":".into()),
                                                ]),
                                            )]),
                                        ),
                                        ("documentFormattingProvider", Value::Bool(true)),
                                        ("textDocumentSync", Value::Number("1".into())),
                                    ]),
                                ),
                                (
                                    "serverInfo",
                                    object([
                                        ("name", Value::String("aml-lsp".into())),
                                        ("version", Value::String(VERSION.into())),
                                    ]),
                                ),
                            ]),
                        ),
                    )?;
                }
            }
            "shutdown" => {
                if let Some(id) = id {
                    write_message(output, &response(id, Value::Null))?;
                }
            }
            "exit" => return Ok(()),
            "textDocument/didOpen" | "textDocument/didChange" => {
                let uri = string(&message, &["params", "textDocument", "uri"])
                    .ok_or("LSP document URI is missing")?
                    .to_string();
                let source = if method.ends_with("didOpen") {
                    string(&message, &["params", "textDocument", "text"])
                        .ok_or("LSP document text is missing")?
                } else {
                    let changes = get(&message, &["params", "contentChanges"]);
                    let Value::Array(changes) = changes.ok_or("LSP changes are missing")? else {
                        return Err("LSP changes must be an array".into());
                    };
                    changes
                        .last()
                        .and_then(|value| value.object().ok()?.get("text")?.string().ok())
                        .ok_or("LSP change text is missing")?
                }
                .to_string();
                documents.insert(uri.clone(), source.clone());
                write_message(
                    output,
                    &notification(
                        "textDocument/publishDiagnostics",
                        object([
                            ("diagnostics", diagnostics(&source)),
                            ("uri", Value::String(uri)),
                        ]),
                    ),
                )?;
            }
            "textDocument/formatting" => {
                if let Some(id) = id {
                    let uri = string(&message, &["params", "textDocument", "uri"])
                        .ok_or("LSP document URI is missing")?;
                    let source = documents.get(uri).map(String::as_str).unwrap_or("");
                    let edit = object([
                        ("newText", Value::String(format_source(source))),
                        (
                            "range",
                            object([
                                (
                                    "end",
                                    object([
                                        ("character", Value::Number("0".into())),
                                        ("line", Value::Number("2147483647".into())),
                                    ]),
                                ),
                                (
                                    "start",
                                    object([
                                        ("character", Value::Number("0".into())),
                                        ("line", Value::Number("0".into())),
                                    ]),
                                ),
                            ]),
                        ),
                    ]);
                    write_message(output, &response(id, Value::Array(vec![edit])))?;
                }
            }
            "textDocument/completion" => {
                if let Some(id) = id {
                    write_message(output, &response(id, completion_items()))?;
                }
            }
            _ => {
                if let Some(id) = id {
                    write_message(output, &response(id, Value::Null))?;
                }
            }
        }
    }
    Ok(())
}

pub fn serve_stdio() -> Result<(), String> {
    let stdin = io::stdin();
    let stdout = io::stdout();
    serve(&mut stdin.lock(), &mut stdout.lock())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    #[test]
    fn framing_round_trip() {
        let value = object([
            ("id", Value::Number("1".into())),
            ("jsonrpc", Value::String("2.0".into())),
        ]);
        let mut bytes = Vec::new();
        write_message(&mut bytes, &value).unwrap();
        assert_eq!(read_message(&mut Cursor::new(bytes)).unwrap(), Some(value));
    }
    #[test]
    fn reports_precise_diagnostic() {
        let Value::Array(items) = diagnostics("app bad\nprofile script\ntask main:\n\tshow 1\n")
        else {
            panic!()
        };
        let range = &items[0].object().unwrap()["range"];
        assert_eq!(
            range.object().unwrap()["start"].object().unwrap()["line"],
            Value::Number("3".into())
        );
    }
}
