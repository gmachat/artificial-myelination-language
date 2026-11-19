"""Stable, dependency-free AML core intermediate representation."""

from __future__ import annotations

import json
import re
from typing import Any

from .compiler import CompileError, NAME, PROFILES
from .expression import ExpressionError, expression_data, parse_expression
from .typesystem import parse_type, split_top_level


def _expr(source: str, line: int) -> dict[str, Any]:
    try:
        return expression_data(parse_expression(source))
    except ExpressionError as error:
        raise CompileError(error.message, line, error.column) from error


def _statement(text: str, line: int) -> dict[str, Any]:
    for keyword in ("show", "fail", "require", "ensure"):
        prefix = keyword + " "
        if text.startswith(prefix):
            source = text[len(prefix) :].strip()
            if not source:
                raise CompileError(f"{keyword} needs an expression", line)
            return {"kind": keyword, "expression": _expr(source, line), "line": line}
    if text == "return":
        return {"kind": "return", "value": None, "line": line}
    if text.startswith("return "):
        return {"kind": "return", "value": _expr(text[7:].strip(), line), "line": line}
    if text.startswith("let "):
        match = re.fullmatch(rf"let\s+({NAME})\s*=\s*(.+)", text)
        if not match:
            raise CompileError("use 'let <name> = <expression>'", line)
        return {"kind": "let", "name": match.group(1), "value": _expr(match.group(2), line), "line": line}
    return {"kind": "expression", "expression": _expr(text, line), "line": line}


def _body(lines: list[tuple[int, int, str]], start: int, indent: int) -> tuple[list[dict[str, Any]], int]:
    body: list[dict[str, Any]] = []
    index = start
    while index < len(lines):
        line, level, text = lines[index]
        if level < indent:
            break
        if level > indent:
            raise CompileError("unexpected indentation", line)
        if text.startswith("if ") and text.endswith(":"):
            condition = text[3:-1].strip()
            then_body, index = _body(lines, index + 1, indent + 1)
            else_body: list[dict[str, Any]] = []
            if index < len(lines) and lines[index][1] == indent and lines[index][2] == "else:":
                else_body, index = _body(lines, index + 1, indent + 1)
            body.append({"kind": "if", "condition": _expr(condition, line), "then": then_body, "else": else_body, "line": line})
            continue
        if text == "else:":
            break
        body.append(_statement(text, line))
        index += 1
    return body, index


def program_ir(source: str, filename: str = "<memory>") -> dict[str, Any]:
    """Parse core AML once for consumption by any language backend."""
    lines: list[tuple[int, int, str]] = []
    for number, raw in enumerate(source.splitlines(), 1):
        if "\t" in raw:
            raise CompileError("tabs are not allowed; use two spaces", number)
        if not raw.strip() or raw.lstrip().startswith("#"):
            continue
        spaces = len(raw) - len(raw.lstrip(" "))
        if spaces % 2:
            raise CompileError("indentation must use multiples of two spaces", number)
        lines.append((number, spaces // 2, raw.strip()))
    if not lines:
        raise CompileError("empty source; expected 'app <name>'", 1)
    first_line, first_indent, first = lines[0]
    app = re.fullmatch(rf"app\s+({NAME})", first)
    if first_indent or not app:
        raise CompileError("the first declaration must be 'app <name>'", first_line)

    result: dict[str, Any] = {"aml_ir": "1.0", "source": filename, "app": app.group(1), "profile": None, "records": [], "tasks": []}
    index = 1
    while index < len(lines):
        line, indent, text = lines[index]
        if indent:
            raise CompileError("unexpected indentation", line)
        if text.startswith("profile "):
            profile = text[8:].strip()
            if profile not in PROFILES:
                raise CompileError(f"unknown profile '{profile}'", line)
            result["profile"] = profile
            index += 1
            continue
        if text.startswith("record "):
            match = re.fullmatch(rf"record\s+({NAME}):", text)
            if not match:
                raise CompileError("use 'record <Name>:'", line)
            fields: list[dict[str, str]] = []
            index += 1
            while index < len(lines) and lines[index][1] == 1:
                field_line, _, field_text = lines[index]
                field = re.fullmatch(rf"({NAME})\s*:\s*(.+)", field_text)
                if not field:
                    raise CompileError("record fields must use 'name: Type'", field_line)
                try:
                    field_type = parse_type(field.group(2)).canonical()
                except ValueError as error:
                    raise CompileError(str(error), field_line) from error
                fields.append({"name": field.group(1), "type": field_type})
                index += 1
            result["records"].append({"name": match.group(1), "fields": fields, "line": line})
            continue
        if text.startswith("task "):
            task = re.fullmatch(rf"task\s+({NAME})(?:\((.*?)\))?(?:\s*->\s*(.+?))?:", text)
            if not task:
                raise CompileError("invalid task declaration", line)
            name, raw_parameters, raw_return = task.groups()
            parameters: list[dict[str, str]] = []
            for raw_parameter in split_top_level(raw_parameters or ""):
                parameter = re.fullmatch(rf"({NAME})\s*:\s*(.+)", raw_parameter)
                if not parameter:
                    raise CompileError("parameters must use 'name: Type'", line)
                try:
                    parameter_type = parse_type(parameter.group(2)).canonical()
                except ValueError as error:
                    raise CompileError(str(error), line) from error
                parameters.append({"name": parameter.group(1), "type": parameter_type})
            try:
                return_type = parse_type(raw_return).canonical() if raw_return else None
            except ValueError as error:
                raise CompileError(str(error), line) from error
            body, index = _body(lines, index + 1, 1)
            result["tasks"].append({"name": name, "parameters": parameters, "return": return_type, "body": body, "line": line})
            continue
        raise CompileError("only profile, record, and task declarations are allowed", line)
    if result["profile"] is None:
        raise CompileError("application must declare a profile", first_line)
    if not result["tasks"]:
        raise CompileError("application must define at least one task", first_line)
    from .semantic import analyze_program

    return analyze_program(result)


def compile_ir(source: str, filename: str = "<memory>") -> str:
    return json.dumps(program_ir(source, filename), indent=2, sort_keys=True) + "\n"
