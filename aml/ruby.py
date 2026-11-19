"""Dependency-free Ruby backend consuming AML's target-neutral IR."""

from __future__ import annotations

import json
from typing import Any

from .ir import program_ir


RUNTIME = r'''class AMLFailure < StandardError
end

def aml_valid(value, expected)
  return true if expected == "Any"
  if expected.start_with?("Optional[")
    return value.nil? || aml_valid(value, expected[9...-1])
  end
  if expected.start_with?("List[")
    return value.is_a?(Array) && value.all? { |item| aml_valid(item, expected[5...-1]) }
  end
  if AML_RECORDS.key?(expected)
    fields = AML_RECORDS[expected]
    return value.is_a?(Hash) && value.keys.sort == fields.keys.sort &&
      fields.all? { |name, type| aml_valid(value[name], type) }
  end
  return value == true || value == false if expected == "Bool"
  return value.instance_of?(Float) if expected == "Float"
  return value.instance_of?(Integer) if expected == "Int"
  return value.instance_of?(String) if expected == "Text"
  false
end

def aml_expect(place, value, expected)
  return if aml_valid(value, expected)
  raise AMLFailure, "type | #{place} expected #{expected}, got #{value.class}"
end

def aml_require(condition, task, kind, expression)
  raise AMLFailure, "contract | #{task} #{kind} failed: #{expression}" unless condition
end'''


def _expr(value: dict[str, Any]) -> str:
    kind = value["kind"]
    if kind == "literal":
        literal = value["value"]
        if literal is None:
            return "nil"
        if literal is True:
            return "true"
        if literal is False:
            return "false"
        return json.dumps(literal, ensure_ascii=False)
    if kind == "name":
        return value["value"]
    if kind == "unary":
        operator = {"not": "!", "+": "+", "-": "-"}[value["operator"]]
        return f"({operator}{_expr(value['value'])})"
    if kind == "binary":
        operator = {"and": "&&", "or": "||"}.get(value["operator"], value["operator"])
        return f"({_expr(value['left'])} {operator} {_expr(value['right'])})"
    if kind == "call":
        arguments = ", ".join(_expr(item) for item in value["arguments"])
        return f"{_expr(value['callee'])}({arguments})"
    if kind == "index":
        return f"{_expr(value['value'])}[{_expr(value['index'])}]"
    if kind == "member":
        return f"{_expr(value['value'])}[{json.dumps(value['name'])}]"
    if kind == "list":
        return "[" + ", ".join(_expr(item) for item in value["items"]) + "]"
    if kind == "record":
        fields = ", ".join(
            f"{json.dumps(item['name'])} => {_expr(item['value'])}" for item in value["fields"]
        )
        return "{" + fields + "}"
    raise ValueError(f"unsupported IR expression: {kind}")


def _body(
    statements: list[dict[str, Any]],
    task: str,
    return_type: str | None,
    level: int = 1,
    ensures: list[dict[str, Any]] | None = None,
) -> list[str]:
    output: list[str] = []
    indent = "  " * level
    if ensures is None:
        ensures = [item["expression"] for item in statements if item["kind"] == "ensure"]
    for statement in statements:
        kind = statement["kind"]
        if kind == "ensure":
            continue
        if kind == "let":
            output.append(f"{indent}{statement['name']} = {_expr(statement['value'])}")
        elif kind == "show":
            output.append(f"{indent}puts({_expr(statement['expression'])})")
        elif kind == "fail":
            output.append(f"{indent}raise AMLFailure, String({_expr(statement['expression'])})")
        elif kind == "require":
            output.append(
                f"{indent}aml_require(!!({_expr(statement['expression'])}), {task!r}, 'require', 'condition')"
            )
        elif kind == "return":
            if statement["value"] is None:
                output.append(f"{indent}return")
                continue
            rendered = _expr(statement["value"])
            if ensures or return_type:
                output.append(f"{indent}result = {rendered}")
                for condition in ensures:
                    output.append(
                        f"{indent}aml_require(!!({_expr(condition)}), {task!r}, 'ensure', 'condition')"
                    )
                if return_type:
                    output.append(f"{indent}aml_expect({f'{task}.result'!r}, result, {return_type!r})")
                output.append(f"{indent}return result")
            else:
                output.append(f"{indent}return {rendered}")
        elif kind == "if":
            output.append(f"{indent}if {_expr(statement['condition'])}")
            output.extend(_body(statement["then"], task, return_type, level + 1, ensures))
            if statement["else"]:
                output.append(f"{indent}else")
                output.extend(_body(statement["else"], task, return_type, level + 1, ensures))
            output.append(f"{indent}end")
        elif kind == "expression":
            output.append(f"{indent}{_expr(statement['expression'])}")
        else:
            raise ValueError(f"unsupported IR statement: {kind}")
    return output or [f"{indent}nil"]


def compile_ruby(source: str, filename: str = "<memory>") -> str:
    ir = program_ir(source, filename)
    records = {
        record["name"]: {field["name"]: field["type"] for field in record["fields"]}
        for record in ir["records"]
    }
    ruby_records = "{" + ", ".join(
        f"{name!r} => {{" + ", ".join(f"{field!r} => {field_type!r}" for field, field_type in fields.items()) + "}"
        for name, fields in records.items()
    ) + "}"
    output = [
        "# Generated by AML 0.8.0.dev0. Do not edit generated code.",
        f"AML_RECORDS = {ruby_records}",
        "",
        RUNTIME,
        "",
    ]
    main_seen = False
    for task in ir["tasks"]:
        parameters = ", ".join(item["name"] for item in task["parameters"])
        output.append(f"def {task['name']}({parameters})")
        for parameter in task["parameters"]:
            place = f"{task['name']}.{parameter['name']}"
            output.append(
                f"  aml_expect({place!r}, {parameter['name']}, {parameter['type']!r})"
            )
        output.extend(_body(task["body"], task["name"], task["return"]))
        output.extend(["end", ""])
        main_seen = main_seen or task["name"] == "main"
    if main_seen:
        output.extend(
            [
                "begin",
                "  main()",
                "rescue AMLFailure => error",
                '  warn "AML_ERROR: #{error.message}"',
                "  exit 1",
                "end",
                "",
            ]
        )
    return "\n".join(output)
