use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExpressionError {
    pub message: String,
    pub column: usize,
}

impl ExpressionError {
    fn new(message: impl Into<String>, column: usize) -> Self {
        Self {
            message: message.into(),
            column,
        }
    }
}

impl fmt::Display for ExpressionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}: {}", self.column, self.message)
    }
}

impl std::error::Error for ExpressionError {}

#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Int(i64),
    Float(f64),
    Text(String),
    Bool(bool),
    None,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Literal(Literal),
    Name(String),
    Unary {
        operator: String,
        value: Box<Expr>,
    },
    Binary {
        left: Box<Expr>,
        operator: String,
        right: Box<Expr>,
    },
    Call {
        callee: Box<Expr>,
        arguments: Vec<Expr>,
    },
    Index {
        value: Box<Expr>,
        index: Box<Expr>,
    },
    Member {
        value: Box<Expr>,
        name: String,
    },
    List(Vec<Expr>),
    Record(Vec<(String, Expr)>),
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum TokenKind {
    Number,
    String,
    Name,
    Operator,
    Eof,
}

#[derive(Debug, Clone)]
struct Token {
    kind: TokenKind,
    value: String,
    column: usize,
}

fn tokenize(source: &str) -> Result<Vec<Token>, ExpressionError> {
    let chars: Vec<char> = source.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        if chars[index].is_whitespace() {
            index += 1;
            continue;
        }
        let column = index + 1;
        if chars[index].is_ascii_digit() {
            let start = index;
            while index < chars.len() && chars[index].is_ascii_digit() {
                index += 1;
            }
            if chars.get(index) == Some(&'.')
                && chars.get(index + 1).is_some_and(char::is_ascii_digit)
            {
                index += 1;
                while index < chars.len() && chars[index].is_ascii_digit() {
                    index += 1;
                }
            }
            tokens.push(Token {
                kind: TokenKind::Number,
                value: chars[start..index].iter().collect(),
                column,
            });
            continue;
        }
        if chars[index] == '"' || chars[index] == '\'' {
            let delimiter = chars[index];
            let start = index;
            index += 1;
            let mut closed = false;
            while index < chars.len() {
                if chars[index] == '\\' {
                    index += 2;
                    continue;
                }
                if chars[index] == delimiter {
                    index += 1;
                    closed = true;
                    break;
                }
                index += 1;
            }
            if !closed {
                return Err(ExpressionError::new("unterminated string", column));
            }
            tokens.push(Token {
                kind: TokenKind::String,
                value: chars[start..index].iter().collect(),
                column,
            });
            continue;
        }
        if chars[index].is_ascii_alphabetic() || chars[index] == '_' {
            let start = index;
            index += 1;
            while index < chars.len()
                && (chars[index].is_ascii_alphanumeric() || chars[index] == '_')
            {
                index += 1;
            }
            tokens.push(Token {
                kind: TokenKind::Name,
                value: chars[start..index].iter().collect(),
                column,
            });
            continue;
        }
        let pair: String = chars[index..usize::min(index + 2, chars.len())]
            .iter()
            .collect();
        if matches!(pair.as_str(), "==" | "!=" | "<=" | ">=") {
            tokens.push(Token {
                kind: TokenKind::Operator,
                value: pair,
                column,
            });
            index += 2;
            continue;
        }
        if "+-*/%<>()[]{},:.".contains(chars[index]) {
            tokens.push(Token {
                kind: TokenKind::Operator,
                value: chars[index].to_string(),
                column,
            });
            index += 1;
            continue;
        }
        return Err(ExpressionError::new(
            format!("unexpected character {:?}", chars[index]),
            column,
        ));
    }
    tokens.push(Token {
        kind: TokenKind::Eof,
        value: String::new(),
        column: chars.len() + 1,
    });
    Ok(tokens)
}

fn decode_string(source: &str, column: usize) -> Result<String, ExpressionError> {
    let content: Vec<char> = source[1..source.len() - 1].chars().collect();
    let mut output = String::new();
    let mut index = 0;
    while index < content.len() {
        if content[index] != '\\' {
            output.push(content[index]);
            index += 1;
            continue;
        }
        index += 1;
        let Some(escaped) = content.get(index) else {
            return Err(ExpressionError::new("invalid string escape", column));
        };
        output.push(match escaped {
            'n' => '\n',
            'r' => '\r',
            't' => '\t',
            '\\' => '\\',
            '"' => '"',
            '\'' => '\'',
            other => *other,
        });
        index += 1;
    }
    Ok(output)
}

fn precedence(operator: &str) -> u8 {
    match operator {
        "or" => 1,
        "and" => 2,
        "==" | "!=" | "<" | "<=" | ">" | ">=" => 3,
        "+" | "-" => 4,
        "*" | "/" | "%" => 5,
        _ => 0,
    }
}

struct Parser {
    tokens: Vec<Token>,
    index: usize,
}

impl Parser {
    fn current(&self) -> &Token {
        &self.tokens[self.index]
    }
    fn advance(&mut self) -> Token {
        let token = self.tokens[self.index].clone();
        self.index += 1;
        token
    }
    fn accept(&mut self, value: &str) -> bool {
        if self.current().value == value {
            self.index += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, value: &str) -> Result<(), ExpressionError> {
        if self.accept(value) {
            Ok(())
        } else {
            Err(ExpressionError::new(
                format!("expected '{value}'"),
                self.current().column,
            ))
        }
    }
    fn parse(mut self) -> Result<Expr, ExpressionError> {
        if self.current().kind == TokenKind::Eof {
            return Err(ExpressionError::new("expected expression", 1));
        }
        let value = self.expression(1)?;
        if self.current().kind != TokenKind::Eof {
            return Err(ExpressionError::new(
                format!("unexpected token '{}'", self.current().value),
                self.current().column,
            ));
        }
        Ok(value)
    }
    fn expression(&mut self, minimum: u8) -> Result<Expr, ExpressionError> {
        let mut left = self.prefix()?;
        loop {
            let operator = self.current().value.clone();
            let level = precedence(&operator);
            if level < minimum {
                break;
            }
            self.advance();
            let right = self.expression(level + 1)?;
            left = Expr::Binary {
                left: Box::new(left),
                operator,
                right: Box::new(right),
            };
        }
        Ok(left)
    }
    fn prefix(&mut self) -> Result<Expr, ExpressionError> {
        let token = self.advance();
        let value = if matches!(token.value.as_str(), "not" | "+" | "-") {
            Expr::Unary {
                operator: token.value,
                value: Box::new(self.expression(6)?),
            }
        } else if token.kind == TokenKind::Number {
            if token.value.contains('.') {
                Expr::Literal(Literal::Float(token.value.parse().unwrap()))
            } else {
                Expr::Literal(Literal::Int(token.value.parse().unwrap()))
            }
        } else if token.kind == TokenKind::String {
            Expr::Literal(Literal::Text(decode_string(&token.value, token.column)?))
        } else if token.kind == TokenKind::Name {
            match token.value.as_str() {
                "true" => Expr::Literal(Literal::Bool(true)),
                "false" => Expr::Literal(Literal::Bool(false)),
                "none" => Expr::Literal(Literal::None),
                _ => Expr::Name(token.value),
            }
        } else if token.value == "(" {
            let nested = self.expression(1)?;
            self.expect(")")?;
            nested
        } else if token.value == "[" {
            let mut items = Vec::new();
            if !self.accept("]") {
                loop {
                    items.push(self.expression(1)?);
                    if self.accept("]") {
                        break;
                    }
                    self.expect(",")?;
                }
            }
            Expr::List(items)
        } else if token.value == "{" {
            let mut fields = Vec::new();
            if !self.accept("}") {
                loop {
                    let key = self.advance();
                    let name = match key.kind {
                        TokenKind::Name => key.value,
                        TokenKind::String => decode_string(&key.value, key.column)?,
                        _ => {
                            return Err(ExpressionError::new(
                                "record keys must be names or strings",
                                key.column,
                            ))
                        }
                    };
                    self.expect(":")?;
                    fields.push((name, self.expression(1)?));
                    if self.accept("}") {
                        break;
                    }
                    self.expect(",")?;
                }
            }
            Expr::Record(fields)
        } else {
            return Err(ExpressionError::new(
                format!("unexpected token '{}'", token.value),
                token.column,
            ));
        };
        self.postfix(value)
    }
    fn postfix(&mut self, mut value: Expr) -> Result<Expr, ExpressionError> {
        loop {
            if self.accept("(") {
                let mut arguments = Vec::new();
                if !self.accept(")") {
                    loop {
                        arguments.push(self.expression(1)?);
                        if self.accept(")") {
                            break;
                        }
                        self.expect(",")?;
                    }
                }
                value = Expr::Call {
                    callee: Box::new(value),
                    arguments,
                };
                continue;
            }
            if self.accept("[") {
                let index = self.expression(1)?;
                self.expect("]")?;
                value = Expr::Index {
                    value: Box::new(value),
                    index: Box::new(index),
                };
                continue;
            }
            if self.accept(".") {
                let member = self.advance();
                if member.kind != TokenKind::Name {
                    return Err(ExpressionError::new(
                        "expected field name after '.'",
                        member.column,
                    ));
                }
                value = Expr::Member {
                    value: Box::new(value),
                    name: member.value,
                };
                continue;
            }
            return Ok(value);
        }
    }
}

pub fn parse(source: &str) -> Result<Expr, ExpressionError> {
    Parser {
        tokens: tokenize(source)?,
        index: 0,
    }
    .parse()
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

fn render(value: &Expr, parent: u8) -> String {
    match value {
        Expr::Literal(Literal::Int(number)) => number.to_string(),
        Expr::Literal(Literal::Float(number)) => {
            let text = number.to_string();
            if text.contains('.') {
                text
            } else {
                format!("{text}.0")
            }
        }
        Expr::Literal(Literal::Text(text)) => quote(text),
        Expr::Literal(Literal::Bool(value)) => value.to_string(),
        Expr::Literal(Literal::None) => "null".into(),
        Expr::Name(name) => name.clone(),
        Expr::Unary { operator, value } => {
            let mapped = if operator == "not" { "!" } else { operator };
            let result = format!("{mapped}{}", render(value, 6));
            if 6 < parent {
                format!("({result})")
            } else {
                result
            }
        }
        Expr::Binary {
            left,
            operator,
            right,
        } => {
            let level = precedence(operator);
            let mapped = match operator.as_str() {
                "and" => "&&",
                "or" => "||",
                "==" => "===",
                "!=" => "!==",
                other => other,
            };
            let result = format!(
                "{} {mapped} {}",
                render(left, level),
                render(right, level + 1)
            );
            if level < parent {
                format!("({result})")
            } else {
                result
            }
        }
        Expr::Call { callee, arguments } => format!(
            "{}({})",
            render(callee, 7),
            arguments
                .iter()
                .map(|item| render(item, 0))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Expr::Index { value, index } => format!("{}[{}]", render(value, 7), render(index, 0)),
        Expr::Member { value, name } => format!("{}.{}", render(value, 7), name),
        Expr::List(items) => format!(
            "[{}]",
            items
                .iter()
                .map(|item| render(item, 0))
                .collect::<Vec<_>>()
                .join(", ")
        ),
        Expr::Record(fields) => format!(
            "{{{}}}",
            fields
                .iter()
                .map(|(name, value)| format!("{}: {}", quote(name), render(value, 0)))
                .collect::<Vec<_>>()
                .join(", ")
        ),
    }
}

pub fn render_javascript(source: &str) -> Result<String, ExpressionError> {
    Ok(render(&parse(source)?, 0))
}

pub fn render_javascript_expr(value: &Expr) -> String {
    render(value, 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn precedence_matches_aml() {
        assert_eq!(render_javascript("1 + 2 * 3").unwrap(), "1 + 2 * 3");
        assert_eq!(render_javascript("(1 + 2) * 3").unwrap(), "(1 + 2) * 3");
        assert_eq!(
            render_javascript("not ready or done").unwrap(),
            "!ready || done"
        );
    }

    #[test]
    fn renders_structured_and_postfix_values() {
        assert_eq!(
            render_javascript("person.scores[0]").unwrap(),
            "person.scores[0]"
        );
        assert_eq!(
            render_javascript("greet(\"AML\", [1, 2])").unwrap(),
            "greet(\"AML\", [1, 2])"
        );
        assert_eq!(
            render_javascript("{name: \"Ada\", active: true}").unwrap(),
            "{\"name\": \"Ada\", \"active\": true}"
        );
    }

    #[test]
    fn reports_precise_columns() {
        let error = parse("1 + @").unwrap_err();
        assert_eq!(error.column, 5);
    }
}
