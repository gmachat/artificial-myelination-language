"""AML-to-React compiler for compact, typed UI components."""

from __future__ import annotations

from dataclasses import dataclass, field
import json
import re

from .compiler import CompileError, NAME, TYPE_MAP
from .expression import ExpressionError, render_expression


JS_TYPES = {name: name for name in TYPE_MAP}

RUNTIME = """\
function __amlActual(value) {
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
}
"""


@dataclass
class Line:
    number: int
    indent: int
    text: str


@dataclass
class ViewNode:
    kind: str
    value: str | None
    line: int
    children: list["ViewNode"] = field(default_factory=list)
    alternate: list["ViewNode"] = field(default_factory=list)


def _expression(source: str) -> str:
    try:
        return render_expression(source, "javascript")
    except ExpressionError as error:
        raise CompileError(error.message, 1, error.column) from error


def _view_expression(source: str) -> str:
    """Support compact interpolation such as \"Count: {count}\"."""
    match = re.fullmatch(r'"((?:[^"\\]|\\.)*)"', source)
    if match and re.search(r"\{[A-Za-z_][A-Za-z0-9_]*\}", match.group(1)):
        content = match.group(1).replace("`", "\\`")
        content = re.sub(r"\{([A-Za-z_][A-Za-z0-9_]*)\}", r"${\1}", content)
        return f"`{content}`"
    return _expression(source)


def _setter(name: str) -> str:
    pieces = name.split("_")
    return "set" + "".join(piece[:1].upper() + piece[1:] for piece in pieces)


def _lines(source: str) -> list[Line]:
    result: list[Line] = []
    for number, raw in enumerate(source.splitlines(), start=1):
        if "\t" in raw:
            raise CompileError("tabs are not allowed; use two spaces", number)
        if not raw.strip() or raw.lstrip().startswith("#"):
            continue
        leading = len(raw) - len(raw.lstrip(" "))
        if leading % 2:
            raise CompileError("indentation must use multiples of two spaces", number)
        result.append(Line(number, leading // 2, raw.strip()))
    return result


def _children(lines: list[Line], start: int, parent_indent: int) -> tuple[list[ViewNode], int]:
    nodes: list[ViewNode] = []
    index = start
    expected = parent_indent + 1
    while index < len(lines):
        line = lines[index]
        if line.indent <= parent_indent:
            break
        if line.indent != expected:
            raise CompileError("unexpected view indentation", line.number)
        text = line.text
        if text in {"column:", "row:"}:
            children, index = _children(lines, index + 1, line.indent)
            if not children:
                raise CompileError(f"{text[:-1]} requires at least one child", line.number)
            nodes.append(ViewNode(text[:-1], None, line.number, children))
            continue
        if text.startswith("when ") and text.endswith(":"):
            condition = text[5:-1].strip()
            if not condition:
                raise CompileError("when needs a condition", line.number)
            children, index = _children(lines, index + 1, line.indent)
            if not children:
                raise CompileError("when requires at least one child", line.number)
            nodes.append(ViewNode("when", condition, line.number, children))
            continue
        if text == "else:":
            if not nodes or nodes[-1].kind != "when" or nodes[-1].alternate:
                raise CompileError("else must immediately follow a when block", line.number)
            children, index = _children(lines, index + 1, line.indent)
            if not children:
                raise CompileError("else requires at least one child", line.number)
            nodes[-1].alternate = children
            continue
        if text.startswith("heading "):
            nodes.append(ViewNode("heading", text[8:].strip(), line.number))
            index += 1
            continue
        if text.startswith("text "):
            nodes.append(ViewNode("text", text[5:].strip(), line.number))
            index += 1
            continue
        if text.startswith("input "):
            name = text[6:].strip()
            if not re.fullmatch(NAME, name):
                raise CompileError("input expects a state name", line.number)
            nodes.append(ViewNode("input", name, line.number))
            index += 1
            continue
        if text.startswith("button ") and text.endswith(":"):
            label = text[7:-1].strip()
            children, index = _children(lines, index + 1, line.indent)
            if len(children) != 1 or children[0].kind != "click":
                raise CompileError("button requires one 'on click' block", line.number)
            nodes.append(ViewNode("button", label, line.number, children))
            continue
        if text == "on click:":
            actions, index = _children(lines, index + 1, line.indent)
            if not actions:
                raise CompileError("on click requires an action", line.number)
            nodes.append(ViewNode("click", None, line.number, actions))
            continue
        if text.startswith("set "):
            match = re.fullmatch(rf"set\s+({NAME})\s*=\s*(.+)", text)
            if not match:
                raise CompileError("use 'set <state> = <expression>'", line.number)
            nodes.append(ViewNode("set", match.group(2), line.number, [ViewNode("name", match.group(1), line.number)]))
            index += 1
            continue
        if text.startswith("show "):
            nodes.append(ViewNode("show", text[5:].strip(), line.number))
            index += 1
            continue
        raise CompileError(f"unknown React view construct: {text}", line.number)
    return nodes, index


def _render_node(node: ViewNode, indent: int, states: dict[str, str]) -> list[str]:
    pad = "  " * indent
    if node.kind in {"column", "row"}:
        direction = "column" if node.kind == "column" else "row"
        output = [
            pad
            + f'<div style={{{{ display: "flex", flexDirection: "{direction}", gap: "0.75rem" }}}}>'
        ]
        for child in node.children:
            output.extend(_render_node(child, indent + 1, states))
        output.append(pad + "</div>")
        return output
    if node.kind == "when":
        condition = _expression(node.value or "")
        operator = "?" if node.alternate else "&&"
        output = [pad + f"{{Boolean({condition}) {operator} (", pad + "  <>"]
        for child in node.children:
            output.extend(_render_node(child, indent + 2, states))
        output.extend([pad + "  </>", pad + ")"])
        if node.alternate:
            output[-1] += " : ("
            output.append(pad + "  <>")
            for child in node.alternate:
                output.extend(_render_node(child, indent + 2, states))
            output.extend([pad + "  </>", pad + ")"])
        output[-1] += "}"
        return output
    if node.kind == "heading":
        return [pad + f"<h1>{{{_view_expression(node.value or '')}}}</h1>"]
    if node.kind == "text":
        return [pad + f"<p>{{{_view_expression(node.value or '')}}}</p>"]
    if node.kind == "input":
        name = node.value or ""
        if name not in states:
            raise CompileError(f"input references unknown state '{name}'", node.line)
        if states[name] != "Text":
            raise CompileError("input state must have type Text", node.line)
        setter = _setter(name)
        return [
            pad
            + f'<input name="{name}" aria-label="{name}" value={{{name}}} '
            + f'onChange={{event => {setter}(event.target.value)}} />'
        ]
    if node.kind == "button":
        actions = node.children[0].children
        rendered_actions: list[str] = []
        for action in actions:
            if action.kind == "set":
                name = action.children[0].value or ""
                if name not in states:
                    raise CompileError(f"set references unknown state '{name}'", action.line)
                rendered_actions.append(f"{_setter(name)}({_expression(action.value or '')});")
            elif action.kind == "show":
                rendered_actions.append(f"console.log({_expression(action.value or '')});")
            else:
                raise CompileError("unsupported click action", action.line)
        action_source = " ".join(rendered_actions)
        label = _view_expression(node.value or '""')
        return [
            pad
            + f'<button type="button" onClick={{() => {{ {action_source} }}}}>{{{label}}}</button>'
        ]
    raise CompileError(f"cannot render React construct '{node.kind}'", node.line)


def compile_react(source: str, filename: str = "<memory>") -> str:
    """Compile one AML React component into a JSX module."""
    del filename
    lines = _lines(source)
    if not lines:
        raise CompileError("empty source; expected 'app <name>'", 1)
    index = 0
    app = re.fullmatch(rf"app\s+({NAME})", lines[index].text)
    if not app or lines[index].indent:
        raise CompileError("the first declaration must be 'app <name>'", lines[index].number)
    index += 1
    valid_profiles = {"profile react", "profile web"}
    if index >= len(lines) or lines[index].text not in valid_profiles or lines[index].indent:
        line = lines[index].number if index < len(lines) else lines[-1].number
        raise CompileError("React target requires 'profile react' or 'profile web'", line)
    index += 1
    if index >= len(lines):
        raise CompileError("React application must define a component", lines[-1].number)

    declaration = lines[index]
    match = re.fullmatch(rf"component\s+({NAME})(?:\((.*?)\))?:", declaration.text)
    if not match or declaration.indent:
        raise CompileError("expected 'component <Name>(props):'", declaration.number)
    component_name, raw_props = match.groups()
    if not component_name[:1].isupper():
        raise CompileError("component names must begin with an uppercase letter", declaration.number)
    props: list[tuple[str, str]] = []
    if raw_props:
        for raw_prop in raw_props.split(","):
            prop = re.fullmatch(rf"\s*({NAME})\s*:\s*({NAME})\s*", raw_prop)
            if not prop:
                raise CompileError("props must use 'name: Type'", declaration.number)
            prop_name, prop_type = prop.groups()
            if prop_type not in TYPE_MAP:
                choices = ", ".join(TYPE_MAP)
                raise CompileError(
                    f"unknown type '{prop_type}'; expected one of: {choices}", declaration.number
                )
            props.append((prop_name, prop_type))
    index += 1

    states: dict[str, str] = {}
    state_values: dict[str, str] = {}
    requirements: list[str] = []
    view_nodes: list[ViewNode] | None = None
    while index < len(lines):
        line = lines[index]
        if line.indent == 0:
            raise CompileError("React 0.3 supports one component per file", line.number)
        if line.indent != 1:
            raise CompileError("unexpected component indentation", line.number)
        state = re.fullmatch(rf"state\s+({NAME})\s*:\s*({NAME})\s*=\s*(.+)", line.text)
        if state:
            name, type_name, value = state.groups()
            if type_name not in TYPE_MAP:
                choices = ", ".join(TYPE_MAP)
                raise CompileError(
                    f"unknown type '{type_name}'; expected one of: {choices}", line.number
                )
            if name in states:
                raise CompileError(f"duplicate state '{name}'", line.number)
            states[name] = type_name
            state_values[name] = value
            index += 1
            continue
        if line.text.startswith("require "):
            condition = line.text[8:].strip()
            if not condition:
                raise CompileError("require needs a condition", line.number)
            requirements.append(condition)
            index += 1
            continue
        if line.text == "view:":
            if view_nodes is not None:
                raise CompileError("component may only have one view", line.number)
            view_nodes, index = _children(lines, index + 1, line.indent)
            if not view_nodes:
                raise CompileError("view requires at least one element", line.number)
            continue
        raise CompileError(f"unknown component construct: {line.text}", line.number)
    if view_nodes is None:
        raise CompileError("component must define a view", declaration.number)
    if len(view_nodes) != 1:
        raise CompileError("view must contain one root element", declaration.number)

    prop_names = ", ".join(name for name, _ in props)
    parameter = f"{{ {prop_names} }}" if props else ""
    output = [
        "// Generated by AML 0.8.0.dev0. Do not edit generated code.",
        'import React, { useState } from "react";',
        "",
        RUNTIME.rstrip(),
        "",
        f"export function {component_name}({parameter}) {{",
    ]
    for name, type_name in props:
        output.append(
            "  " + f"__amlExpect({json.dumps(f'{component_name}.{name}')}, {name}, {json.dumps(JS_TYPES[type_name])});"
        )
    for name, type_name in states.items():
        output.append(
            "  "
            + f"const [{name}, {_setter(name)}] = useState(() => "
            + f"__amlExpect({json.dumps(f'{component_name}.{name}')}, "
            + f"{_expression(state_values[name])}, {json.dumps(JS_TYPES[type_name])}));"
        )
        output.append(
            "  " + f"__amlExpect({json.dumps(f'{component_name}.{name}')}, {name}, {json.dumps(JS_TYPES[type_name])});"
        )
    for condition in requirements:
        output.append(
            "  "
            + f"__amlRequire(Boolean({_expression(condition)}), "
            + f"{json.dumps(component_name)}, {json.dumps(condition)});"
        )
    output.extend(["", "  return ("])
    output.extend(_render_node(view_nodes[0], 2, states))
    output.extend(["  );", "}", "", f"export default {component_name};", ""])
    return "\n".join(output)

