"""Target-neutral AML expression tokenizer, AST, parser, and renderers."""

from __future__ import annotations

from dataclasses import dataclass
import json
import re
from typing import Any


@dataclass(frozen=True)
class ExpressionError(ValueError):
    message: str
    column: int

    def __str__(self) -> str:
        return f"{self.column}: {self.message}"


@dataclass(frozen=True)
class Token:
    kind: str
    value: str
    column: int


class Expr:
    pass


@dataclass(frozen=True)
class Literal(Expr):
    value: Any
    kind: str


@dataclass(frozen=True)
class Name(Expr):
    value: str


@dataclass(frozen=True)
class Unary(Expr):
    operator: str
    operand: Expr


@dataclass(frozen=True)
class Binary(Expr):
    left: Expr
    operator: str
    right: Expr


@dataclass(frozen=True)
class Call(Expr):
    callee: Expr
    arguments: tuple[Expr, ...]


@dataclass(frozen=True)
class Index(Expr):
    value: Expr
    index: Expr


@dataclass(frozen=True)
class Member(Expr):
    value: Expr
    name: str


@dataclass(frozen=True)
class ListLiteral(Expr):
    items: tuple[Expr, ...]


@dataclass(frozen=True)
class RecordLiteral(Expr):
    fields: tuple[tuple[str, Expr], ...]


TOKEN = re.compile(
    r"\s*(?:(?P<number>\d+(?:\.\d+)?)|(?P<string>\"(?:[^\"\\]|\\.)*\"|'(?:[^'\\]|\\.)*')|(?P<name>[A-Za-z_][A-Za-z0-9_]*)|(?P<operator>==|!=|<=|>=|[+\-*/%<>()\[\]{},:.]))"
)


def tokenize(source: str) -> list[Token]:
    tokens: list[Token] = []
    position = 0
    while position < len(source):
        match = TOKEN.match(source, position)
        if not match:
            if source[position:].strip() == "":
                break
            raise ExpressionError(f"unexpected character {source[position]!r}", position + 1)
        kind = match.lastgroup or ""
        value = match.group(kind)
        column = match.start(kind) + 1
        tokens.append(Token(kind, value, column))
        position = match.end()
    tokens.append(Token("eof", "", len(source) + 1))
    return tokens


PRECEDENCE = {
    "or": 1,
    "and": 2,
    "==": 3,
    "!=": 3,
    "<": 3,
    "<=": 3,
    ">": 3,
    ">=": 3,
    "+": 4,
    "-": 4,
    "*": 5,
    "/": 5,
    "%": 5,
}


class Parser:
    def __init__(self, source: str):
        self.tokens = tokenize(source)
        self.index = 0

    @property
    def current(self) -> Token:
        return self.tokens[self.index]

    def advance(self) -> Token:
        token = self.current
        self.index += 1
        return token

    def accept(self, value: str) -> bool:
        if self.current.value == value:
            self.advance()
            return True
        return False

    def expect(self, value: str) -> None:
        if not self.accept(value):
            raise ExpressionError(f"expected '{value}'", self.current.column)

    def parse(self) -> Expr:
        if self.current.kind == "eof":
            raise ExpressionError("expected expression", 1)
        expression = self.expression(1)
        if self.current.kind != "eof":
            raise ExpressionError(f"unexpected token '{self.current.value}'", self.current.column)
        return expression

    def expression(self, minimum: int) -> Expr:
        left = self.prefix()
        while True:
            operator = self.current.value
            precedence = PRECEDENCE.get(operator, 0)
            if precedence < minimum:
                break
            self.advance()
            right = self.expression(precedence + 1)
            left = Binary(left, operator, right)
        return left

    def prefix(self) -> Expr:
        token = self.advance()
        if token.value in {"not", "+", "-"}:
            value = Unary(token.value, self.expression(6))
            return self.postfix(value)
        if token.kind == "number":
            value: Expr = Literal(float(token.value) if "." in token.value else int(token.value), "number")
            return self.postfix(value)
        if token.kind == "string":
            try:
                decoded = json.loads(token.value) if token.value.startswith('"') else _single_string(token.value)
            except (ValueError, json.JSONDecodeError) as error:
                raise ExpressionError(f"invalid string: {error}", token.column) from error
            return self.postfix(Literal(decoded, "string"))
        if token.kind == "name":
            if token.value in {"true", "false", "none"}:
                values = {"true": True, "false": False, "none": None}
                return self.postfix(Literal(values[token.value], "constant"))
            return self.postfix(Name(token.value))
        if token.value == "(":
            value = self.expression(1)
            self.expect(")")
            return self.postfix(value)
        if token.value == "[":
            items: list[Expr] = []
            if not self.accept("]"):
                while True:
                    items.append(self.expression(1))
                    if self.accept("]"):
                        break
                    self.expect(",")
            return self.postfix(ListLiteral(tuple(items)))
        if token.value == "{":
            fields: list[tuple[str, Expr]] = []
            if not self.accept("}"):
                while True:
                    key = self.advance()
                    if key.kind == "name":
                        field_name = key.value
                    elif key.kind == "string":
                        field_name = json.loads(key.value) if key.value.startswith('"') else _single_string(key.value)
                    else:
                        raise ExpressionError("record keys must be names or strings", key.column)
                    self.expect(":")
                    fields.append((field_name, self.expression(1)))
                    if self.accept("}"):
                        break
                    self.expect(",")
            return self.postfix(RecordLiteral(tuple(fields)))
        raise ExpressionError(f"unexpected token '{token.value}'", token.column)

    def postfix(self, value: Expr) -> Expr:
        while True:
            if self.accept("("):
                arguments: list[Expr] = []
                if not self.accept(")"):
                    while True:
                        arguments.append(self.expression(1))
                        if self.accept(")"):
                            break
                        self.expect(",")
                value = Call(value, tuple(arguments))
                continue
            if self.accept("["):
                index = self.expression(1)
                self.expect("]")
                value = Index(value, index)
                continue
            if self.accept("."):
                member = self.advance()
                if member.kind != "name":
                    raise ExpressionError("expected field name after '.'", member.column)
                value = Member(value, member.value)
                continue
            return value


def _single_string(source: str) -> str:
    content = source[1:-1]
    return bytes(content, "utf-8").decode("unicode_escape")


def parse_expression(source: str) -> Expr:
    return Parser(source).parse()


def expression_data(expression: Expr) -> dict[str, Any]:
    """Convert an expression AST to AML's stable, target-neutral data form."""
    if isinstance(expression, Literal):
        return {"kind": "literal", "literal": expression.kind, "value": expression.value}
    if isinstance(expression, Name):
        return {"kind": "name", "value": expression.value}
    if isinstance(expression, Unary):
        return {"kind": "unary", "operator": expression.operator, "value": expression_data(expression.operand)}
    if isinstance(expression, Binary):
        return {
            "kind": "binary",
            "operator": expression.operator,
            "left": expression_data(expression.left),
            "right": expression_data(expression.right),
        }
    if isinstance(expression, Call):
        return {"kind": "call", "callee": expression_data(expression.callee), "arguments": [expression_data(item) for item in expression.arguments]}
    if isinstance(expression, Index):
        return {"kind": "index", "value": expression_data(expression.value), "index": expression_data(expression.index)}
    if isinstance(expression, Member):
        return {"kind": "member", "value": expression_data(expression.value), "name": expression.name}
    if isinstance(expression, ListLiteral):
        return {"kind": "list", "items": [expression_data(item) for item in expression.items]}
    if isinstance(expression, RecordLiteral):
        return {"kind": "record", "fields": [{"name": name, "value": expression_data(value)} for name, value in expression.fields]}
    raise TypeError(f"unknown expression: {expression!r}")


TARGET_OPERATORS = {
    "python": {"and": "and", "or": "or", "not": "not"},
    "javascript": {"and": "&&", "or": "||", "not": "!", "==": "===", "!=": "!=="},
}


def render_expression(source: str, target: str) -> str:
    if target not in TARGET_OPERATORS:
        raise ValueError(f"unknown expression target: {target}")
    return _render(parse_expression(source), target, 0)


def _render(expression: Expr, target: str, parent: int) -> str:
    operators = TARGET_OPERATORS[target]
    if isinstance(expression, Literal):
        if expression.kind == "string":
            return json.dumps(expression.value, ensure_ascii=False)
        if expression.value is True:
            return "True" if target == "python" else "true"
        if expression.value is False:
            return "False" if target == "python" else "false"
        if expression.value is None:
            return "None" if target == "python" else "null"
        return repr(expression.value)
    if isinstance(expression, Name):
        return expression.value
    if isinstance(expression, Unary):
        precedence = 6
        operator = operators.get(expression.operator, expression.operator)
        separator = " " if operator == "not" else ""
        result = f"{operator}{separator}{_render(expression.operand, target, precedence)}"
        return f"({result})" if precedence < parent else result
    if isinstance(expression, Binary):
        precedence = PRECEDENCE[expression.operator]
        operator = operators.get(expression.operator, expression.operator)
        result = (
            f"{_render(expression.left, target, precedence)} {operator} "
            f"{_render(expression.right, target, precedence + 1)}"
        )
        return f"({result})" if precedence < parent else result
    if isinstance(expression, Call):
        return f"{_render(expression.callee, target, 7)}({', '.join(_render(item, target, 0) for item in expression.arguments)})"
    if isinstance(expression, Index):
        return f"{_render(expression.value, target, 7)}[{_render(expression.index, target, 0)}]"
    if isinstance(expression, Member):
        base = _render(expression.value, target, 7)
        return f"{base}[{expression.name!r}]" if target == "python" else f"{base}.{expression.name}"
    if isinstance(expression, ListLiteral):
        return f"[{', '.join(_render(item, target, 0) for item in expression.items)}]"
    if isinstance(expression, RecordLiteral):
        fields = ", ".join(
            f"{json.dumps(name)}: {_render(value, target, 0)}" for name, value in expression.fields
        )
        return f"{{{fields}}}"
    raise TypeError(f"unknown expression: {expression!r}")

