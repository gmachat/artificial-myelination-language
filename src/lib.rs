pub mod agent;
pub mod benchmark;
pub mod c_backend;
pub mod core;
pub mod csharp;
pub mod diagnose;
pub mod expression;
pub mod external;
pub mod fullstack;
pub mod go;
pub mod ir;
pub mod java;
pub mod javascript;
pub mod json;
pub mod lsp;
pub mod mcp;
pub mod modules;
pub mod packages;
pub mod python;
pub mod react;
pub mod ruby;
pub mod rust_backend;
pub mod scaffold;
pub mod semantic;
pub mod sha256;
pub mod swift;
pub mod test_runner;
pub mod types;
pub mod web;

use std::fmt;

pub const VERSION: &str = "0.8.0.dev0-rust1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileError {
    pub message: String,
    pub line: usize,
    pub column: usize,
}

impl CompileError {
    pub fn new(message: impl Into<String>, line: usize) -> Self {
        Self {
            message: message.into(),
            line,
            column: 1,
        }
    }
}

impl fmt::Display for CompileError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}: {}", self.line, self.column, self.message)
    }
}

impl std::error::Error for CompileError {}

pub fn format_source(source: &str) -> String {
    let mut output: Vec<String> = Vec::new();
    let mut blank = false;
    for raw in source.lines() {
        let expanded = raw.replace('\t', "  ");
        let trimmed_end = expanded.trim_end();
        if trimmed_end.trim().is_empty() {
            if !output.is_empty() && !blank {
                output.push(String::new());
            }
            blank = true;
            continue;
        }
        let leading = trimmed_end.len() - trimmed_end.trim_start_matches(' ').len();
        let normalized = leading.div_ceil(2) * 2;
        output.push(format!(
            "{}{}",
            " ".repeat(normalized),
            trimmed_end.trim_start()
        ));
        blank = false;
    }
    while output.last().is_some_and(|line| line.is_empty()) {
        output.pop();
    }
    if output.is_empty() {
        String::new()
    } else {
        output.join("\n") + "\n"
    }
}

pub const REFERENCE: &str = include_str!("../aml/reference.txt");
