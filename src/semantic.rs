use std::collections::{BTreeMap, HashMap};

use crate::{
    expression::{Expr, Literal},
    types::TypeRef,
};

#[derive(Debug, Clone)]
pub struct TaskSignature {
    pub parameters: Vec<TypeRef>,
    pub result: Option<TypeRef>,
}

#[derive(Debug, Default)]
pub struct Analyzer {
    pub records: BTreeMap<String, BTreeMap<String, TypeRef>>,
    pub tasks: BTreeMap<String, TaskSignature>,
}

fn scalar(name: &str) -> TypeRef {
    TypeRef {
        name: name.into(),
        arguments: Vec::new(),
    }
}

pub fn compatible(actual: &TypeRef, expected: &TypeRef) -> bool {
    if actual.name == "Any" || expected.name == "Any" || actual == expected {
        return true;
    }
    expected.name == "Optional"
        && (actual.name == "None"
            || expected
                .arguments
                .first()
                .is_some_and(|inner| compatible(actual, inner)))
}

impl Analyzer {
    pub fn infer(
        &self,
        value: &Expr,
        names: &HashMap<String, TypeRef>,
        allow_void: bool,
    ) -> Result<TypeRef, String> {
        match value {
            Expr::Literal(Literal::Int(_)) => Ok(scalar("Int")),
            Expr::Literal(Literal::Float(_)) => Ok(scalar("Float")),
            Expr::Literal(Literal::Text(_)) => Ok(scalar("Text")),
            Expr::Literal(Literal::Bool(_)) => Ok(scalar("Bool")),
            Expr::Literal(Literal::None) => Ok(scalar("None")),
            Expr::Name(name) => names
                .get(name)
                .cloned()
                .ok_or_else(|| format!("unknown name '{name}'")),
            Expr::Unary { operator, value } => {
                let operand = self.infer(value, names, false)?;
                if operator == "not" {
                    if operand.name != "Bool" {
                        return Err(format!("not expects Bool, got {}", operand.canonical()));
                    }
                    Ok(scalar("Bool"))
                } else if matches!(operand.name.as_str(), "Int" | "Float") {
                    Ok(operand)
                } else {
                    Err(format!(
                        "unary {operator} expects a number, got {}",
                        operand.canonical()
                    ))
                }
            }
            Expr::Binary {
                left,
                operator,
                right,
            } => {
                let left = self.infer(left, names, false)?;
                let right = self.infer(right, names, false)?;
                let left_name = left.canonical();
                let right_name = right.canonical();
                match operator.as_str() {
                    "and" | "or" => {
                        if left.name != "Bool" || right.name != "Bool" {
                            Err(format!("{operator} expects Bool operands, got {left_name} and {right_name}"))
                        } else {
                            Ok(scalar("Bool"))
                        }
                    }
                    "==" | "!=" => {
                        if compatible(&left, &right) || compatible(&right, &left) {
                            Ok(scalar("Bool"))
                        } else {
                            Err(format!("cannot compare {left_name} and {right_name}"))
                        }
                    }
                    "<" | "<=" | ">" | ">=" => {
                        let numeric = matches!(left.name.as_str(), "Int" | "Float")
                            && matches!(right.name.as_str(), "Int" | "Float");
                        if numeric || (left.name == "Text" && right.name == "Text") {
                            Ok(scalar("Bool"))
                        } else {
                            Err(format!(
                                "{operator} does not support {left_name} and {right_name}"
                            ))
                        }
                    }
                    "+" if left.name == "Text" && right.name == "Text" => Ok(scalar("Text")),
                    "+" | "-" | "*" | "/" | "%" => {
                        let numeric = matches!(left.name.as_str(), "Int" | "Float")
                            && matches!(right.name.as_str(), "Int" | "Float");
                        if !numeric {
                            return Err(format!(
                                "{operator} expects numbers, got {left_name} and {right_name}"
                            ));
                        }
                        if operator == "%" && (left.name != "Int" || right.name != "Int") {
                            return Err("% expects Int operands".into());
                        }
                        if operator == "/" || left.name == "Float" || right.name == "Float" {
                            Ok(scalar("Float"))
                        } else {
                            Ok(scalar("Int"))
                        }
                    }
                    _ => Err(format!("unknown operator '{operator}'")),
                }
            }
            Expr::Call { callee, arguments } => {
                let Expr::Name(name) = callee.as_ref() else {
                    return Err("calls must reference a known AML task".into());
                };
                let Some(signature) = self.tasks.get(name) else {
                    return Err("calls must reference a known AML task".into());
                };
                if arguments.len() != signature.parameters.len() {
                    return Err(format!(
                        "task '{name}' expects {} arguments, got {}",
                        signature.parameters.len(),
                        arguments.len()
                    ));
                }
                for (index, (argument, expected)) in
                    arguments.iter().zip(&signature.parameters).enumerate()
                {
                    let actual = self.infer(argument, names, false)?;
                    if !compatible(&actual, expected) {
                        return Err(format!(
                            "argument {} of '{name}' expects {}, got {}",
                            index + 1,
                            expected.canonical(),
                            actual.canonical()
                        ));
                    }
                }
                match &signature.result {
                    Some(result) => Ok(result.clone()),
                    None if allow_void => Ok(scalar("Void")),
                    None => Err(format!("task '{name}' does not return a value")),
                }
            }
            Expr::Index { value, index } => {
                let container = self.infer(value, names, false)?;
                let index = self.infer(index, names, false)?;
                if index.name != "Int" {
                    return Err(format!(
                        "{} index expects Int, got {}",
                        if container.name == "Text" {
                            "text"
                        } else {
                            "list"
                        },
                        index.canonical()
                    ));
                }
                if container.name == "List" {
                    Ok(container.arguments[0].clone())
                } else if container.name == "Text" {
                    Ok(scalar("Text"))
                } else {
                    Err(format!("cannot index {}", container.canonical()))
                }
            }
            Expr::Member { value, name } => {
                let owner = self.infer(value, names, false)?;
                let Some(fields) = self.records.get(&owner.name) else {
                    return Err(format!(
                        "field access expects a record, got {}",
                        owner.canonical()
                    ));
                };
                fields
                    .get(name)
                    .cloned()
                    .ok_or_else(|| format!("record {} has no field '{name}'", owner.name))
            }
            Expr::List(items) => {
                if items.is_empty() {
                    return Ok(TypeRef {
                        name: "List".into(),
                        arguments: vec![scalar("Any")],
                    });
                }
                let mut item_type = self.infer(&items[0], names, false)?;
                for item in &items[1..] {
                    let other = self.infer(item, names, false)?;
                    if item_type == other {
                        continue;
                    }
                    if matches!(item_type.name.as_str(), "Int" | "Float")
                        && matches!(other.name.as_str(), "Int" | "Float")
                    {
                        item_type = scalar("Float");
                    } else {
                        return Err(format!(
                            "list mixes {} and {}",
                            item_type.canonical(),
                            other.canonical()
                        ));
                    }
                }
                Ok(TypeRef {
                    name: "List".into(),
                    arguments: vec![item_type],
                })
            }
            Expr::Record(supplied) => {
                let mut supplied_map = BTreeMap::new();
                for (name, value) in supplied {
                    if supplied_map.insert(name, value).is_some() {
                        return Err("record literal contains duplicate fields".into());
                    }
                }
                let mut candidates = Vec::new();
                'records: for (record_name, fields) in &self.records {
                    if fields.len() != supplied_map.len()
                        || !fields.keys().all(|name| supplied_map.contains_key(name))
                    {
                        continue;
                    }
                    for (name, expected) in fields {
                        let actual = self.infer(supplied_map[name], names, false)?;
                        if !compatible(&actual, expected) {
                            continue 'records;
                        }
                    }
                    candidates.push(record_name.clone());
                }
                match candidates.as_slice() {
                    [] => Err("record literal does not match a declared record".into()),
                    [name] => Ok(scalar(name)),
                    _ => Err(format!(
                        "ambiguous record literal; matches: {}",
                        candidates.join(", ")
                    )),
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{expression, types};

    #[test]
    fn infers_numeric_and_list_expressions() {
        let analyzer = Analyzer::default();
        let names = HashMap::new();
        assert_eq!(
            analyzer
                .infer(&expression::parse("1 + 2.5").unwrap(), &names, false)
                .unwrap()
                .canonical(),
            "Float"
        );
        assert_eq!(
            analyzer
                .infer(&expression::parse("[1, 2]").unwrap(), &names, false)
                .unwrap()
                .canonical(),
            "List[Int]"
        );
    }

    #[test]
    fn checks_calls_and_records() {
        let mut analyzer = Analyzer::default();
        analyzer.tasks.insert(
            "double".into(),
            TaskSignature {
                parameters: vec![types::parse("Int").unwrap()],
                result: Some(types::parse("Int").unwrap()),
            },
        );
        analyzer.records.insert(
            "Person".into(),
            BTreeMap::from([("name".into(), types::parse("Text").unwrap())]),
        );
        let names = HashMap::new();
        assert_eq!(
            analyzer
                .infer(&expression::parse("double(2)").unwrap(), &names, false)
                .unwrap()
                .canonical(),
            "Int"
        );
        assert_eq!(
            analyzer
                .infer(
                    &expression::parse("{name: \"Ada\"}").unwrap(),
                    &names,
                    false
                )
                .unwrap()
                .canonical(),
            "Person"
        );
    }
}
