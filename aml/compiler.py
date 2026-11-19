"""Core AML compiler and Python backend."""

from __future__ import annotations

from dataclasses import dataclass
import re

from .expression import ExpressionError, render_expression
from .typesystem import TypeRef, parse_type, python_annotation, split_top_level, validate_type


NAME = r"[A-Za-z_][A-Za-z0-9_]*"
TYPE_MAP = {
    "Any": "object",
    "Bool": "bool",
    "Float": "float",
    "Int": "int",
    "Text": "str",
}
PROFILES = {
    "script": ("console",),
    "local": ("console", "files"),
    "connected": ("console", "files", "network"),
    "react": ("browser", "ui"),
    "web": ("browser", "ui", "network"),
    "fullstack": ("browser", "ui", "network", "server", "storage"),
}

RUNTIME = """\
class AMLFailure(Exception):
    pass


def __aml_valid(value: object, expected: str) -> bool:
    if expected == \"Any\":
        return True
    if expected.startswith(\"Optional[\"):
        return value is None or __aml_valid(value, expected[9:-1])
    if expected.startswith(\"List[\"):
        return isinstance(value, list) and all(__aml_valid(item, expected[5:-1]) for item in value)
    if expected in __aml_records__:
        fields = __aml_records__[expected]
        return isinstance(value, dict) and set(value) == set(fields) and all(
            __aml_valid(value[name], field_type) for name, field_type in fields.items()
        )
    scalar = {\"Bool\": bool, \"Float\": float, \"Int\": int, \"Text\": str}.get(expected)
    return scalar is not None and type(value) is scalar


def __aml_expect(place: str, value: object, expected: str) -> None:
    if not __aml_valid(value, expected):
        actual = type(value).__name__
        display = {\"Bool\": \"bool\", \"Float\": \"float\", \"Int\": \"int\", \"Text\": \"str\"}.get(expected, expected)
        raise AMLFailure(f\"type | {place} expected {display}, got {actual}\")


def __aml_require(condition: bool, task: str, kind: str, expression: str) -> None:
    if not condition:
        raise AMLFailure(f\"contract | {task} {kind} failed: {expression}\")
"""


@dataclass
class CompileError(Exception):
    message: str
    line: int
    column: int = 1

    def __str__(self) -> str:
        return f"{self.line}:{self.column}: {self.message}"


def _expression(source: str) -> str:
    """Parse and render one target-neutral AML expression."""
    try:
        return render_expression(source, "python")
    except ExpressionError as error:
        raise CompileError(error.message, 1, error.column) from error


def _type(source: str, line: int, records: set[str]) -> TypeRef:
    try:
        value = parse_type(source)
        validate_type(value, records)
        return value
    except ValueError as error:
        raise CompileError(str(error), line) from error


def _task_signature(text: str, line: int, records: set[str]) -> tuple[str, str, list[tuple[str, str]], str | None]:
    match = re.fullmatch(
        rf"task\s+({NAME})(?:\((.*?)\))?(?:\s*->\s*(.+?))?:", text
    )
    if not match:
        raise CompileError("invalid task declaration", line)

    name, raw_parameters, raw_return = match.groups()
    parameters: list[str] = []
    typed_parameters: list[tuple[str, str]] = []
    if raw_parameters:
        for parameter in split_top_level(raw_parameters):
            item = re.fullmatch(rf"\s*({NAME})\s*:\s*(.+?)\s*", parameter)
            if not item:
                raise CompileError("parameters must use 'name: Type'", line)
            param_name, param_type = item.groups()
            parsed = _type(param_type, line, records)
            parameters.append(f"{param_name}: {python_annotation(parsed)}")
            typed_parameters.append((param_name, parsed.canonical()))

    parsed_return = _type(raw_return, line, records) if raw_return else None
    return_type = python_annotation(parsed_return) if parsed_return else "None"
    return name, f"def {name}({', '.join(parameters)}) -> {return_type}:", typed_parameters, (parsed_return.canonical() if parsed_return else None)


def compile_source(
    source: str, filename: str = "<memory>", target: str = "python"
) -> str:
    """Compile one AML source file for the selected backend."""
    if target in {"python", "javascript", "js"}:
        from .ir import program_ir

        program_ir(source, filename)
    if target in {"javascript", "js"}:
        from .javascript import compile_javascript

        return compile_javascript(source, filename)
    if target in {"react", "web"}:
        from .react import compile_react

        return compile_react(source, filename)
    if target == "ir":
        from .ir import compile_ir

        return compile_ir(source, filename)
    if target == "ruby":
        from .ruby import compile_ruby

        return compile_ruby(source, filename)
    if target == "go":
        from .go_backend import compile_go

        return compile_go(source, filename)
    if target in {"rust", "c", "java", "csharp", "cs", "swift"}:
        from .static_backends import compile_static

        return compile_static(source, filename, "csharp" if target == "cs" else target)
    if target != "python":
        raise ValueError(f"unknown compilation target: {target}")
    del filename  # Reserved for future multi-file diagnostics.
    output = [
        "# Generated by AML 0.8.0.dev0. Do not edit generated code.",
        "from __future__ import annotations",
        "import sys",
        "",
    ]
    app_seen = False
    profile_name: str | None = None
    task_seen = False
    main_seen = False
    current_indent = 0
    block_kinds: dict[int, str] = {}
    current_task: str | None = None
    current_return_type: str | None = None
    current_ensures: list[str] = []
    current_has_return = False
    contracts: dict[str, dict[str, list[str]]] = {}
    records: dict[str, dict[str, str]] = {}
    current_record: str | None = None

    def finish_task(line: int) -> None:
        nonlocal current_task, current_return_type, current_ensures, current_has_return
        if current_task and current_ensures and not current_has_return:
            raise CompileError(
                f"task '{current_task}' has ensure contracts but no return", line
            )
        if current_task and current_return_type and not current_has_return:
            raise CompileError(
                f"task '{current_task}' declares a return type but has no return", line
            )
        current_task = None
        current_return_type = None
        current_ensures = []
        current_has_return = False

    lines = source.splitlines()
    for line_number, raw in enumerate(lines, start=1):
        if "\t" in raw:
            raise CompileError("tabs are not allowed; use two spaces", line_number)
        if not raw.strip() or raw.lstrip().startswith("#"):
            continue

        leading = len(raw) - len(raw.lstrip(" "))
        if leading % 2:
            raise CompileError("indentation must use multiples of two spaces", line_number)
        indent = leading // 2
        text = raw.strip()

        if not app_seen:
            app = re.fullmatch(rf"app\s+({NAME})", text)
            if not app or indent:
                raise CompileError("the first declaration must be 'app <name>'", line_number)
            app_seen = True
            output.extend([f"__aml_app__ = {app.group(1)!r}", ""])
            continue

        if indent == 0 and text.startswith("record "):
            if task_seen:
                raise CompileError("records must appear before the first task", line_number)
            match = re.fullmatch(rf"record\s+({NAME}):", text)
            if not match:
                raise CompileError("use 'record <Name>:'", line_number)
            current_record = match.group(1)
            if current_record in records:
                raise CompileError(f"duplicate record '{current_record}'", line_number)
            records[current_record] = {}
            continue

        if current_record and indent == 1:
            field = re.fullmatch(rf"({NAME})\s*:\s*(.+)", text)
            if not field:
                raise CompileError("record fields must use 'name: Type'", line_number)
            field_name, field_type = field.groups()
            if field_name in records[current_record]:
                raise CompileError(f"duplicate field '{field_name}'", line_number)
            try:
                records[current_record][field_name] = parse_type(field_type).canonical()
            except ValueError as error:
                raise CompileError(str(error), line_number) from error
            continue
        if indent == 0:
            current_record = None

        if indent == 0 and text.startswith("profile "):
            if task_seen:
                raise CompileError("profile must appear before the first task", line_number)
            if profile_name:
                raise CompileError("only one profile is allowed", line_number)
            profile = re.fullmatch(rf"profile\s+({NAME})", text)
            if not profile or profile.group(1) not in PROFILES:
                choices = ", ".join(PROFILES)
                raise CompileError(f"unknown profile; expected one of: {choices}", line_number)
            profile_name = profile.group(1)
            output.extend(
                [
                    f"__aml_profile__ = {profile_name!r}",
                    f"__aml_capabilities__ = {PROFILES[profile_name]!r}",
                    "",
                    RUNTIME.rstrip(),
                    "",
                ]
            )
            continue

        if indent > current_indent + 1:
            raise CompileError("indentation skipped a block level", line_number)
        if indent and indent not in block_kinds:
            raise CompileError("unexpected indentation", line_number)
        current_indent = indent
        python_indent = "    " * indent

        if indent == 0 and text.startswith("task "):
            if not profile_name:
                raise CompileError("declare a profile before the first task", line_number)
            finish_task(line_number)
            for record_name, fields in records.items():
                if not fields:
                    raise CompileError(f"record '{record_name}' needs at least one field", line_number)
                for field_type in fields.values():
                    _type(field_type, line_number, set(records))
            task_name, signature, parameters, return_type = _task_signature(
                text, line_number, set(records)
            )
            if task_name == "main":
                if main_seen:
                    raise CompileError("only one main task is allowed", line_number)
                if signature != "def main() -> None:":
                    raise CompileError("main cannot have parameters or a return type", line_number)
                main_seen = True
            task_seen = True
            current_task = task_name
            current_return_type = return_type
            contracts[task_name] = {"require": [], "ensure": []}
            output.append(signature)
            for parameter_name, parameter_type in parameters:
                output.append(
                    "    "
                    + f"__aml_expect({f'{task_name}.{parameter_name}'!r}, "
                    + f"{parameter_name}, {parameter_type!r})"
                )
            block_kinds = {1: "task"}
            current_indent = 0
            continue

        if indent == 0:
            raise CompileError("only task declarations are allowed at application level", line_number)
        if not task_seen:
            raise CompileError("statements must be inside a task", line_number)

        if text.startswith("require "):
            condition = text[8:].strip()
            if not condition:
                raise CompileError("require needs a condition", line_number)
            contracts[current_task]["require"].append(condition)  # type: ignore[index]
            rendered = (
                f"__aml_require(bool({_expression(condition)}), {current_task!r}, "
                f"'require', {condition!r})"
            )
        elif text.startswith("ensure "):
            if indent != 1:
                raise CompileError("ensure must be at the top level of a task", line_number)
            condition = text[7:].strip()
            if not condition:
                raise CompileError("ensure needs a condition", line_number)
            if "result" not in condition:
                raise CompileError("ensure must describe 'result'", line_number)
            current_ensures.append(condition)
            contracts[current_task]["ensure"].append(condition)  # type: ignore[index]
            continue
        elif text.startswith("let "):
            match = re.fullmatch(rf"let\s+({NAME})\s*=\s*(.+)", text)
            if not match:
                raise CompileError("use 'let <name> = <expression>'", line_number)
            name, value = match.groups()
            rendered = f"{name} = {_expression(value)}"
        elif text.startswith("show "):
            rendered = f"print({_expression(text[5:].strip())})"
        elif text == "return":
            if current_ensures:
                raise CompileError("a task with ensure contracts must return a value", line_number)
            rendered = "return"
        elif text.startswith("return "):
            value = _expression(text[7:].strip())
            current_has_return = True
            if current_ensures or current_return_type:
                output.append(python_indent + f"result = {value}")
                for condition in current_ensures:
                    output.append(
                        python_indent
                        + f"__aml_require(bool({_expression(condition)}), {current_task!r}, "
                        + f"'ensure', {condition!r})"
                    )
                if current_return_type:
                    output.append(
                        python_indent
                        + f"__aml_expect({f'{current_task}.result'!r}, result, "
                        + f"{current_return_type!r})"
                    )
                rendered = "return result"
            else:
                rendered = f"return {value}"
        elif text.startswith("fail "):
            rendered = f"raise AMLFailure(str({_expression(text[5:].strip())}))"
        elif text.startswith("if ") and text.endswith(":"):
            condition = text[3:-1].strip()
            if not condition:
                raise CompileError("if requires a condition", line_number)
            rendered = f"if {_expression(condition)}:"
            block_kinds[indent] = "if"
            block_kinds[indent + 1] = "if-body"
        elif text == "else:":
            if block_kinds.get(indent) != "if":
                raise CompileError("else must follow an if block", line_number)
            rendered = "else:"
            block_kinds[indent] = "else"
            block_kinds[indent + 1] = "else-body"
        else:
            rendered = _expression(text)

        output.append(python_indent + rendered)

    if not app_seen:
        raise CompileError("empty source; expected 'app <name>'", 1)
    finish_task(max(1, len(lines)))
    if not profile_name:
        raise CompileError("application must declare a profile", max(1, len(lines)))
    if not task_seen:
        raise CompileError("application must define at least one task", max(1, len(lines)))
    output.extend(["", f"__aml_records__ = {records!r}", f"__aml_contracts__ = {contracts!r}"])
    if main_seen:
        output.extend(
            [
                "",
                "if __name__ == \"__main__\":",
                "    try:",
                "        main()",
                "    except AMLFailure as error:",
                "        print(f\"AML_ERROR: {error}\", file=sys.stderr)",
                "        raise SystemExit(1)",
            ]
        )
    output.append("")
    return "\n".join(output)

