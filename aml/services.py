"""Shared language services used by the CLI, editors, and AI agents."""

from __future__ import annotations

from dataclasses import asdict, dataclass
import json
from pathlib import Path
from typing import Callable


KEYWORDS = (
    "app",
    "module",
    "use",
    "profile",
    "task",
    "let",
    "show",
    "require",
    "ensure",
    "return",
    "fail",
    "if",
    "when",
    "else",
    "record",
    "data",
    "resource",
    "page",
    "storage",
    "component",
    "state",
    "view",
)
TYPE_NAMES = ("Any", "Bool", "Float", "Int", "Text", "List[T]", "Optional[T]", "Time")


@dataclass(frozen=True)
class Diagnostic:
    path: str
    line: int
    column: int
    message: str
    code: str = "AML1000"
    severity: str = "error"

    def to_dict(self) -> dict[str, object]:
        return asdict(self)


def format_source(source: str) -> str:
    """Apply AML's conservative canonical whitespace format."""
    formatted: list[str] = []
    blank = False
    for raw in source.splitlines():
        raw = raw.replace("\t", "  ").rstrip()
        if not raw.strip():
            if formatted and not blank:
                formatted.append("")
            blank = True
            continue
        leading = len(raw) - len(raw.lstrip(" "))
        normalized = " " * (round(leading / 2) * 2) + raw.lstrip(" ")
        formatted.append(normalized)
        blank = False
    while formatted and not formatted[-1]:
        formatted.pop()
    return "\n".join(formatted) + ("\n" if formatted else "")


def diagnostic_for_error(path: str | Path, error: Exception) -> Diagnostic:
    line = int(getattr(error, "line", 1))
    column = int(getattr(error, "column", 1))
    message = str(getattr(error, "message", error))
    return Diagnostic(str(path), line, column, message)


def diagnostic_json(diagnostics: list[Diagnostic]) -> str:
    return json.dumps(
        {"valid": not diagnostics, "diagnostics": [item.to_dict() for item in diagnostics]},
        indent=2,
        sort_keys=True,
    )


def compact_reference() -> str:
    """Return the canonical small-context reference for coding agents."""
    return """AML AI REFERENCE 0.8
Goal: one obvious form, two-space indentation, deterministic defaults.

CORE
app name
profile script|local|connected
use "relative/module.aml"
task name(arg: Text, count: Int) -> Text:
  require count >= 0
  ensure result != ""
  let value = expression
  if condition:
    show value
  else:
    fail "message"
  return value
module files begin with: module name
Types: Any Bool Float Int Text List[T] Optional[T] and named records.
record Person:
  name: Text
  tags: List[Text]
Literals: true false none; lists [a, b]; records {name: "Ada"}; fields person.name.
Commands: aml check|run|build|format FILE; aml targets; build --backend EXECUTABLE. Core targets include python javascript ruby go rust c java csharp swift; plus react web fullstack ir.
IR 1.1 is typed; backends consume resolved types instead of inferring them.

REACT
profile react
component Counter(title: Text):
  state count: Int = 0
  view:
    column:
      heading title
      text "Count: {count}"
      when count == 0:
        text "Ready"
      else:
        text "Started"
      button "Add":
        on click:
          set count = count + 1
      input name

FULLSTACK
profile fullstack
storage json|postgres
data Game:
  league: Text
  starts_at: Time
resource games: Game:
  list
  create
  update
  delete
page Dashboard at "/":
  resource games
Persistent types: Text Int Float Bool Time. id/timestamps are generated.
Postgres command: npm run migrate. Never invent alternate AML syntax.
"""


def reference_json() -> str:
    return json.dumps(
        {
            "version": "0.8",
            "indent": 2,
            "keywords": KEYWORDS,
            "types": TYPE_NAMES,
            "reference": compact_reference(),
        },
        indent=2,
    )


def completion_items() -> list[dict[str, object]]:
    items = [
        {"label": keyword, "kind": 14, "detail": "AML keyword"} for keyword in KEYWORDS
    ]
    items.extend(
        {"label": name, "kind": 7, "detail": "AML type"} for name in TYPE_NAMES
    )
    return items


def check_text(
    source: str,
    path: str,
    compiler: Callable[[str, str], str],
) -> list[Diagnostic]:
    try:
        compiler(source, path)
        return []
    except Exception as error:
        return [diagnostic_for_error(path, error)]

