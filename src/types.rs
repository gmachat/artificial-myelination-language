use std::{collections::BTreeSet, fmt};

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TypeRef {
    pub name: String,
    pub arguments: Vec<TypeRef>,
}

impl TypeRef {
    pub fn canonical(&self) -> String {
        if self.arguments.is_empty() {
            self.name.clone()
        } else {
            format!(
                "{}[{}]",
                self.name,
                self.arguments
                    .iter()
                    .map(TypeRef::canonical)
                    .collect::<Vec<_>>()
                    .join(", ")
            )
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TypeError(pub String);

impl fmt::Display for TypeError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}
impl std::error::Error for TypeError {}

struct Parser<'a> {
    source: &'a str,
    position: usize,
}

impl<'a> Parser<'a> {
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
    fn name(&mut self) -> Result<String, TypeError> {
        self.whitespace();
        let mut end = self.position;
        for (offset, value) in self.source[self.position..].char_indices() {
            if offset == 0 && !(value.is_ascii_alphabetic() || value == '_') {
                break;
            }
            if offset > 0 && !(value.is_ascii_alphanumeric() || value == '_') {
                break;
            }
            end = self.position + offset + value.len_utf8();
        }
        if end == self.position {
            return Err(TypeError("expected type name".into()));
        }
        let value = self.source[self.position..end].to_string();
        self.position = end;
        Ok(value)
    }
    fn kind(&mut self) -> Result<TypeRef, TypeError> {
        let name = self.name()?;
        self.whitespace();
        let mut arguments = Vec::new();
        if self.source[self.position..].starts_with('[') {
            self.position += 1;
            arguments.push(self.kind()?);
            self.whitespace();
            while self.source[self.position..].starts_with(',') {
                self.position += 1;
                arguments.push(self.kind()?);
                self.whitespace();
            }
            if !self.source[self.position..].starts_with(']') {
                return Err(TypeError("expected ']' in type".into()));
            }
            self.position += 1;
        }
        Ok(TypeRef { name, arguments })
    }
}

pub fn parse(source: &str) -> Result<TypeRef, TypeError> {
    let mut parser = Parser {
        source,
        position: 0,
    };
    let value = parser.kind()?;
    parser.whitespace();
    if parser.position != source.len() {
        return Err(TypeError(format!(
            "unexpected type syntax: {}",
            &source[parser.position..]
        )));
    }
    if matches!(value.name.as_str(), "List" | "Optional") && value.arguments.len() != 1 {
        return Err(TypeError(format!(
            "{} requires one type argument",
            value.name
        )));
    }
    if !matches!(value.name.as_str(), "List" | "Optional") && !value.arguments.is_empty() {
        return Err(TypeError(format!(
            "{} does not accept type arguments",
            value.name
        )));
    }
    Ok(value)
}

pub fn validate(value: &TypeRef, records: &BTreeSet<String>) -> Result<(), TypeError> {
    let known = matches!(
        value.name.as_str(),
        "Any" | "Bool" | "Float" | "Int" | "Text" | "List" | "Optional"
    ) || records.contains(&value.name);
    if !known {
        let mut choices: BTreeSet<String> =
            ["Any", "Bool", "Float", "Int", "List", "Optional", "Text"]
                .into_iter()
                .map(str::to_string)
                .collect();
        choices.extend(records.iter().cloned());
        return Err(TypeError(format!(
            "unknown type '{}'; expected one of: {}",
            value.name,
            choices.into_iter().collect::<Vec<_>>().join(", ")
        )));
    }
    for argument in &value.arguments {
        validate(argument, records)?;
    }
    Ok(())
}

pub fn split_top_level(source: &str) -> Vec<String> {
    let mut result = Vec::new();
    let mut start = 0;
    let mut depth = 0;
    for (index, character) in source.char_indices() {
        match character {
            '[' => depth += 1,
            ']' => depth -= 1,
            ',' if depth == 0 => {
                result.push(source[start..index].trim().to_string());
                start = index + 1;
            }
            _ => {}
        }
    }
    let tail = source[start..].trim();
    if !tail.is_empty() {
        result.push(tail.to_string());
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn parses_nested_types() {
        assert_eq!(
            parse("Optional[List[Text]]").unwrap().canonical(),
            "Optional[List[Text]]"
        );
    }
    #[test]
    fn validates_records() {
        let records = BTreeSet::from(["Person".to_string()]);
        validate(&parse("List[Person]").unwrap(), &records).unwrap();
        assert!(validate(&parse("Missing").unwrap(), &records).is_err());
    }
    #[test]
    fn splits_nested_parameters() {
        assert_eq!(
            split_top_level("a: List[Int], b: Optional[Text]"),
            ["a: List[Int]", "b: Optional[Text]"]
        );
    }
}
