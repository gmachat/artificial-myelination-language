"""Canonical AML type syntax shared by compiler backends."""

from __future__ import annotations

from dataclasses import dataclass
import re


SCALARS = {"Any", "Bool", "Float", "Int", "Text"}
CONTAINERS = {"List", "Optional"}


@dataclass(frozen=True)
class TypeRef:
    name: str
    arguments: tuple["TypeRef", ...] = ()

    def canonical(self) -> str:
        if not self.arguments:
            return self.name
        return f"{self.name}[{', '.join(item.canonical() for item in self.arguments)}]"


class TypeParser:
    def __init__(self, source: str):
        self.source = source
        self.position = 0

    def whitespace(self) -> None:
        while self.position < len(self.source) and self.source[self.position].isspace():
            self.position += 1

    def name(self) -> str:
        self.whitespace()
        match = re.match(r"[A-Za-z_][A-Za-z0-9_]*", self.source[self.position :])
        if not match:
            raise ValueError("expected type name")
        self.position += len(match.group(0))
        return match.group(0)

    def type(self) -> TypeRef:
        name = self.name()
        self.whitespace()
        arguments: list[TypeRef] = []
        if self.position < len(self.source) and self.source[self.position] == "[":
            self.position += 1
            arguments.append(self.type())
            self.whitespace()
            while self.position < len(self.source) and self.source[self.position] == ",":
                self.position += 1
                arguments.append(self.type())
                self.whitespace()
            if self.position >= len(self.source) or self.source[self.position] != "]":
                raise ValueError("expected ']' in type")
            self.position += 1
        return TypeRef(name, tuple(arguments))

    def parse(self) -> TypeRef:
        result = self.type()
        self.whitespace()
        if self.position != len(self.source):
            raise ValueError(f"unexpected type syntax: {self.source[self.position:]}")
        return result


def parse_type(source: str) -> TypeRef:
    result = TypeParser(source).parse()
    if result.name in CONTAINERS and len(result.arguments) != 1:
        raise ValueError(f"{result.name} requires one type argument")
    if result.name not in CONTAINERS and result.arguments:
        raise ValueError(f"{result.name} does not accept type arguments")
    return result


def validate_type(value: TypeRef, records: set[str]) -> None:
    if value.name not in SCALARS | CONTAINERS | records:
        choices = ", ".join(sorted(SCALARS | CONTAINERS | records))
        raise ValueError(f"unknown type '{value.name}'; expected one of: {choices}")
    for argument in value.arguments:
        validate_type(argument, records)


def python_annotation(value: TypeRef) -> str:
    scalars = {"Any": "object", "Bool": "bool", "Float": "float", "Int": "int", "Text": "str"}
    if value.name in scalars:
        return scalars[value.name]
    if value.name == "List":
        return f"list[{python_annotation(value.arguments[0])}]"
    if value.name == "Optional":
        return f"{python_annotation(value.arguments[0])} | None"
    return "dict[str, object]"


def split_top_level(source: str) -> list[str]:
    result: list[str] = []
    start = 0
    depth = 0
    for index, character in enumerate(source):
        if character == "[":
            depth += 1
        elif character == "]":
            depth -= 1
        elif character == "," and depth == 0:
            result.append(source[start:index].strip())
            start = index + 1
    tail = source[start:].strip()
    if tail:
        result.append(tail)
    return result

