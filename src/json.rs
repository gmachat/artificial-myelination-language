use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(String),
    String(String),
    Array(Vec<Value>),
    Object(BTreeMap<String, Value>),
}

impl Value {
    pub fn object(&self) -> Result<&BTreeMap<String, Value>, String> {
        match self {
            Self::Object(value) => Ok(value),
            _ => Err("expected JSON object".into()),
        }
    }
    pub fn string(&self) -> Result<&str, String> {
        match self {
            Self::String(value) => Ok(value),
            _ => Err("expected JSON string".into()),
        }
    }
}

struct Parser<'a> {
    source: &'a str,
    position: usize,
}

impl Parser<'_> {
    fn whitespace(&mut self) {
        while self.source[self.position..]
            .chars()
            .next()
            .is_some_and(char::is_whitespace)
        {
            self.position += self.source[self.position..]
                .chars()
                .next()
                .unwrap()
                .len_utf8();
        }
    }
    fn take(&mut self, expected: char) -> Result<(), String> {
        self.whitespace();
        if self.source[self.position..].starts_with(expected) {
            self.position += expected.len_utf8();
            Ok(())
        } else {
            Err(format!("expected '{expected}' at byte {}", self.position))
        }
    }
    fn string(&mut self) -> Result<String, String> {
        self.take('"')?;
        let mut output = String::new();
        while self.position < self.source.len() {
            let value = self.source[self.position..].chars().next().unwrap();
            self.position += value.len_utf8();
            match value {
                '"' => return Ok(output),
                '\\' => {
                    let escaped = self.source[self.position..]
                        .chars()
                        .next()
                        .ok_or("unfinished JSON escape")?;
                    self.position += escaped.len_utf8();
                    match escaped {
                        '"' | '\\' | '/' => output.push(escaped),
                        'b' => output.push('\u{0008}'),
                        'f' => output.push('\u{000c}'),
                        'n' => output.push('\n'),
                        'r' => output.push('\r'),
                        't' => output.push('\t'),
                        'u' => {
                            if self.position + 4 > self.source.len() {
                                return Err("unfinished Unicode escape".into());
                            }
                            let raw = &self.source[self.position..self.position + 4];
                            self.position += 4;
                            let code = u32::from_str_radix(raw, 16)
                                .map_err(|_| "invalid Unicode escape")?;
                            output.push(char::from_u32(code).ok_or("invalid Unicode code point")?);
                        }
                        _ => return Err("invalid JSON escape".into()),
                    }
                }
                value if value.is_control() => {
                    return Err("control character in JSON string".into())
                }
                value => output.push(value),
            }
        }
        Err("unfinished JSON string".into())
    }
    fn value(&mut self) -> Result<Value, String> {
        self.whitespace();
        let rest = &self.source[self.position..];
        if rest.starts_with('"') {
            return self.string().map(Value::String);
        }
        if rest.starts_with('{') {
            self.position += 1;
            let mut output = BTreeMap::new();
            self.whitespace();
            if self.source[self.position..].starts_with('}') {
                self.position += 1;
                return Ok(Value::Object(output));
            }
            loop {
                let key = self.string()?;
                self.take(':')?;
                let value = self.value()?;
                if output.insert(key.clone(), value).is_some() {
                    return Err(format!("duplicate JSON key '{key}'"));
                }
                self.whitespace();
                if self.source[self.position..].starts_with('}') {
                    self.position += 1;
                    break;
                }
                self.take(',')?;
            }
            return Ok(Value::Object(output));
        }
        if rest.starts_with('[') {
            self.position += 1;
            let mut output = Vec::new();
            self.whitespace();
            if self.source[self.position..].starts_with(']') {
                self.position += 1;
                return Ok(Value::Array(output));
            }
            loop {
                output.push(self.value()?);
                self.whitespace();
                if self.source[self.position..].starts_with(']') {
                    self.position += 1;
                    break;
                }
                self.take(',')?;
            }
            return Ok(Value::Array(output));
        }
        for (text, value) in [
            ("true", Value::Bool(true)),
            ("false", Value::Bool(false)),
            ("null", Value::Null),
        ] {
            if rest.starts_with(text) {
                self.position += text.len();
                return Ok(value);
            }
        }
        let end = rest
            .find(|value: char| value.is_whitespace() || matches!(value, ',' | ']' | '}'))
            .unwrap_or(rest.len());
        let number = &rest[..end];
        if !number.is_empty() && number.parse::<f64>().is_ok() {
            self.position += end;
            return Ok(Value::Number(number.into()));
        }
        Err(format!("invalid JSON value at byte {}", self.position))
    }
}

pub fn parse(source: &str) -> Result<Value, String> {
    let mut parser = Parser {
        source,
        position: 0,
    };
    let value = parser.value()?;
    parser.whitespace();
    if parser.position != source.len() {
        return Err(format!("unexpected JSON at byte {}", parser.position));
    }
    Ok(value)
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

fn render(value: &Value, level: usize) -> String {
    match value {
        Value::Null => "null".into(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.clone(),
        Value::String(value) => quote(value),
        Value::Array(values) if values.is_empty() => "[]".into(),
        Value::Array(values) => format!(
            "[\n{}\n{}]",
            values
                .iter()
                .map(|value| format!("{}{}", "  ".repeat(level + 1), render(value, level + 1)))
                .collect::<Vec<_>>()
                .join(",\n"),
            "  ".repeat(level)
        ),
        Value::Object(values) if values.is_empty() => "{}".into(),
        Value::Object(values) => format!(
            "{{\n{}\n{}}}",
            values
                .iter()
                .map(|(key, value)| format!(
                    "{}{}: {}",
                    "  ".repeat(level + 1),
                    quote(key),
                    render(value, level + 1)
                ))
                .collect::<Vec<_>>()
                .join(",\n"),
            "  ".repeat(level)
        ),
    }
}

fn render_compact(value: &Value) -> String {
    match value {
        Value::Null => "null".into(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => value.clone(),
        Value::String(value) => quote(value),
        Value::Array(values) => format!(
            "[{}]",
            values
                .iter()
                .map(render_compact)
                .collect::<Vec<_>>()
                .join(",")
        ),
        Value::Object(values) => format!(
            "{{{}}}",
            values
                .iter()
                .map(|(key, value)| format!("{}:{}", quote(key), render_compact(value)))
                .collect::<Vec<_>>()
                .join(",")
        ),
    }
}

pub fn pretty(value: &Value) -> String {
    render(value, 0) + "\n"
}

pub fn compact(value: &Value) -> String {
    render_compact(value)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn round_trip() {
        let source = "{\"name\":\"demo\",\"dependencies\":{\"one\":{\"path\":\"../one\"}}}";
        assert_eq!(
            parse(&pretty(&parse(source).unwrap())).unwrap(),
            parse(source).unwrap()
        );
    }

    #[test]
    fn compact_is_one_line_and_round_trips() {
        let source = "{\"message\":\"hello\",\"values\":[1,true,null]}";
        let rendered = compact(&parse(source).unwrap());
        assert!(!rendered.contains('\n'));
        assert_eq!(parse(&rendered).unwrap(), parse(source).unwrap());
    }
}
