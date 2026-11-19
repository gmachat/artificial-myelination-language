"""Shared semantic analysis and type annotation for AML IR."""

from __future__ import annotations

from typing import Any

from .compiler import CompileError
from .typesystem import TypeRef, parse_type, validate_type


NUMERIC = {"Int", "Float"}


def _ref(source: str) -> TypeRef:
    if source == "None":
        return TypeRef("None")
    return parse_type(source)


def _compatible(actual: str, expected: str) -> bool:
    if expected == "Any" or actual == "Any" or actual == expected:
        return True
    expected_ref = _ref(expected)
    if expected_ref.name == "Optional":
        return actual == "None" or _compatible(actual, expected_ref.arguments[0].canonical())
    return False


class SemanticAnalyzer:
    def __init__(self, program: dict[str, Any]):
        self.program = program
        record_names = [record["name"] for record in program["records"]]
        if len(record_names) != len(set(record_names)):
            duplicate = next(name for name in record_names if record_names.count(name) > 1)
            raise CompileError(f"duplicate record '{duplicate}'", 1)
        task_names = [task["name"] for task in program["tasks"]]
        if len(task_names) != len(set(task_names)):
            duplicate = next(name for name in task_names if task_names.count(name) > 1)
            raise CompileError(f"duplicate task '{duplicate}'", 1)
        known_records = set(record_names)
        for record in program["records"]:
            fields = [field["name"] for field in record["fields"]]
            if len(fields) != len(set(fields)):
                duplicate = next(name for name in fields if fields.count(name) > 1)
                raise CompileError(f"duplicate field '{duplicate}'", record["line"])
            for field in record["fields"]:
                try:
                    validate_type(parse_type(field["type"]), known_records)
                except ValueError as error:
                    raise CompileError(str(error), record["line"]) from error
        for task in program["tasks"]:
            parameters = [parameter["name"] for parameter in task["parameters"]]
            if len(parameters) != len(set(parameters)):
                duplicate = next(name for name in parameters if parameters.count(name) > 1)
                raise CompileError(f"duplicate parameter '{duplicate}'", task["line"])
            declarations = [parameter["type"] for parameter in task["parameters"]]
            if task["return"]:
                declarations.append(task["return"])
            for declaration in declarations:
                try:
                    validate_type(parse_type(declaration), known_records)
                except ValueError as error:
                    raise CompileError(str(error), task["line"]) from error
        self.records: dict[str, dict[str, str]] = {
            record["name"]: {field["name"]: field["type"] for field in record["fields"]}
            for record in program["records"]
        }
        self.tasks: dict[str, tuple[list[str], str | None]] = {
            task["name"]: ([parameter["type"] for parameter in task["parameters"]], task["return"])
            for task in program["tasks"]
        }

    def error(self, message: str, line: int) -> None:
        raise CompileError(message, line)

    def expression(
        self, value: dict[str, Any], env: dict[str, str], line: int, allow_void: bool = False
    ) -> str:
        kind = value["kind"]
        if kind == "literal":
            literal = value["value"]
            if literal is None:
                result = "None"
            elif isinstance(literal, bool):
                result = "Bool"
            elif isinstance(literal, int):
                result = "Int"
            elif isinstance(literal, float):
                result = "Float"
            elif isinstance(literal, str):
                result = "Text"
            else:
                self.error("unsupported literal", line)
        elif kind == "name":
            name = value["value"]
            if name not in env:
                self.error(f"unknown name '{name}'", line)
            result = env[name]
        elif kind == "unary":
            operand = self.expression(value["value"], env, line)
            operator = value["operator"]
            if operator == "not":
                if operand != "Bool":
                    self.error(f"not expects Bool, got {operand}", line)
                result = "Bool"
            else:
                if operand not in NUMERIC:
                    self.error(f"unary {operator} expects a number, got {operand}", line)
                result = operand
        elif kind == "binary":
            left = self.expression(value["left"], env, line)
            right = self.expression(value["right"], env, line)
            operator = value["operator"]
            if operator in {"and", "or"}:
                if left != "Bool" or right != "Bool":
                    self.error(f"{operator} expects Bool operands, got {left} and {right}", line)
                result = "Bool"
            elif operator in {"==", "!="}:
                if not (_compatible(left, right) or _compatible(right, left)):
                    self.error(f"cannot compare {left} and {right}", line)
                result = "Bool"
            elif operator in {"<", "<=", ">", ">="}:
                if not ((left in NUMERIC and right in NUMERIC) or left == right == "Text"):
                    self.error(f"{operator} does not support {left} and {right}", line)
                result = "Bool"
            elif operator == "+" and left == right == "Text":
                result = "Text"
            elif operator in {"+", "-", "*", "/", "%"}:
                if left not in NUMERIC or right not in NUMERIC:
                    self.error(f"{operator} expects numbers, got {left} and {right}", line)
                if operator == "%" and (left != "Int" or right != "Int"):
                    self.error("% expects Int operands", line)
                result = "Float" if operator == "/" or "Float" in {left, right} else "Int"
            else:
                self.error(f"unknown operator '{operator}'", line)
        elif kind == "call":
            callee = value["callee"]
            if callee["kind"] != "name" or callee["value"] not in self.tasks:
                self.error("calls must reference a known AML task", line)
            name = callee["value"]
            parameters, return_type = self.tasks[name]
            arguments = value["arguments"]
            if len(arguments) != len(parameters):
                self.error(f"task '{name}' expects {len(parameters)} arguments, got {len(arguments)}", line)
            for index, (argument, expected) in enumerate(zip(arguments, parameters), 1):
                actual = self.expression(argument, env, line)
                if not _compatible(actual, expected):
                    self.error(
                        f"argument {index} of '{name}' expects {expected}, got {actual}", line
                    )
            if return_type is None and not allow_void:
                self.error(f"task '{name}' does not return a value", line)
            result = return_type or "Void"
            callee["type"] = f"Task[{', '.join(parameters)} -> {return_type}]"
        elif kind == "index":
            container = self.expression(value["value"], env, line)
            index = self.expression(value["index"], env, line)
            container_ref = _ref(container)
            if container_ref.name == "List":
                if index != "Int":
                    self.error(f"list index expects Int, got {index}", line)
                result = container_ref.arguments[0].canonical()
            elif container == "Text":
                if index != "Int":
                    self.error(f"text index expects Int, got {index}", line)
                result = "Text"
            else:
                self.error(f"cannot index {container}", line)
        elif kind == "member":
            owner = self.expression(value["value"], env, line)
            if owner not in self.records:
                self.error(f"field access expects a record, got {owner}", line)
            name = value["name"]
            if name not in self.records[owner]:
                self.error(f"record {owner} has no field '{name}'", line)
            result = self.records[owner][name]
        elif kind == "list":
            item_types = [self.expression(item, env, line) for item in value["items"]]
            if not item_types:
                result = "List[Any]"
            else:
                item_type = item_types[0]
                for other in item_types[1:]:
                    if item_type == other:
                        continue
                    if item_type in NUMERIC and other in NUMERIC:
                        item_type = "Float"
                    else:
                        self.error(f"list mixes {item_type} and {other}", line)
                result = f"List[{item_type}]"
        elif kind == "record":
            supplied = {field["name"]: field for field in value["fields"]}
            if len(supplied) != len(value["fields"]):
                self.error("record literal contains duplicate fields", line)
            candidates: list[str] = []
            for record_name, fields in self.records.items():
                if set(fields) != set(supplied):
                    continue
                valid = True
                for field_name, expected in fields.items():
                    actual = self.expression(supplied[field_name]["value"], env, line)
                    if not _compatible(actual, expected):
                        valid = False
                        break
                if valid:
                    candidates.append(record_name)
            if not candidates:
                self.error("record literal does not match a declared record", line)
            if len(candidates) > 1:
                self.error(f"ambiguous record literal; matches: {', '.join(candidates)}", line)
            result = candidates[0]
        else:
            self.error(f"unknown expression kind '{kind}'", line)
        value["type"] = result
        return result

    def statements(
        self,
        statements: list[dict[str, Any]],
        env: dict[str, str],
        task_name: str,
        return_type: str | None,
    ) -> None:
        for statement in statements:
            line = statement["line"]
            kind = statement["kind"]
            if kind == "let":
                actual = self.expression(statement["value"], env, line)
                env[statement["name"]] = actual
                statement["type"] = actual
            elif kind in {"show", "fail"}:
                actual = self.expression(statement["expression"], env, line)
                if kind == "fail" and actual != "Text":
                    self.error(f"fail expects Text, got {actual}", line)
            elif kind in {"require", "ensure"}:
                contract_env = dict(env)
                if kind == "ensure":
                    if return_type is None:
                        self.error("ensure requires a declared return type", line)
                    contract_env["result"] = return_type
                actual = self.expression(statement["expression"], contract_env, line)
                if actual != "Bool":
                    self.error(f"{kind} expects Bool, got {actual}", line)
            elif kind == "return":
                if statement["value"] is None:
                    if return_type is not None:
                        self.error(f"task '{task_name}' must return {return_type}", line)
                else:
                    actual = self.expression(statement["value"], env, line)
                    if return_type is not None and not _compatible(actual, return_type):
                        self.error(
                            f"task '{task_name}' returns {actual}, expected {return_type}", line
                        )
            elif kind == "if":
                actual = self.expression(statement["condition"], env, line)
                if actual != "Bool":
                    self.error(f"if expects Bool, got {actual}", line)
                self.statements(statement["then"], dict(env), task_name, return_type)
                self.statements(statement["else"], dict(env), task_name, return_type)
            elif kind == "expression":
                self.expression(statement["expression"], env, line, allow_void=True)

    def analyze(self) -> dict[str, Any]:
        for task in self.program["tasks"]:
            env = {parameter["name"]: parameter["type"] for parameter in task["parameters"]}
            self.statements(task["body"], env, task["name"], task["return"])
        self.program["aml_ir"] = "1.1"
        self.program["semantic"] = "typed"
        return self.program


def analyze_program(program: dict[str, Any]) -> dict[str, Any]:
    return SemanticAnalyzer(program).analyze()
