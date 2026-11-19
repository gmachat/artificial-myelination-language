"""Deterministic local module loading for core AML programs."""

from __future__ import annotations

from pathlib import Path
import re

from .compiler import CompileError


USE = re.compile(r'^use\s+"([^"]+)"\s*$')
DECLARATION = re.compile(r"^(task|record)\s+([A-Za-z_][A-Za-z0-9_]*)")


def load_program(path: Path) -> str:
    """Resolve local `use` declarations into one compiler input."""
    entry = path.resolve()
    visited: set[Path] = set()
    active: list[Path] = []
    module_bodies: list[str] = []
    declarations: dict[str, Path] = {}

    def visit(current: Path, is_entry: bool) -> str:
        current = current.resolve()
        if current in active:
            cycle = " -> ".join(item.name for item in [*active, current])
            raise CompileError(f"module cycle: {cycle}", 1)
        if current in visited:
            return ""
        if not current.is_file():
            raise CompileError(f"module not found: {current}", 1)
        active.append(current)
        lines = current.read_text(encoding="utf-8").splitlines()
        body: list[str] = []
        header_seen = False
        for number, raw in enumerate(lines, start=1):
            text = raw.strip()
            if not text or text.startswith("#"):
                body.append(raw)
                continue
            if not header_seen:
                expected = "app " if is_entry else "module "
                if not text.startswith(expected):
                    raise CompileError(
                        f"{'entry' if is_entry else 'module'} file must begin with '{expected.strip()} <name>'",
                        number,
                    )
                header_seen = True
                if is_entry:
                    body.append(raw)
                continue
            use = USE.fullmatch(text)
            if use:
                if raw[: len(raw) - len(raw.lstrip(" "))]:
                    raise CompileError("use declarations must be at application level", number)
                imported = (current.parent / use.group(1)).resolve()
                imported_body = visit(imported, False)
                if imported_body:
                    module_bodies.append(imported_body)
                continue
            declaration = DECLARATION.match(text) if not raw.startswith(" ") else None
            if declaration:
                name = declaration.group(2)
                previous = declarations.get(name)
                if previous:
                    raise CompileError(
                        f"duplicate declaration '{name}' in {previous.name} and {current.name}",
                        number,
                    )
                declarations[name] = current
            body.append(raw)
        active.pop()
        visited.add(current)
        if not header_seen:
            raise CompileError("empty AML module", 1)
        return "\n".join(body).strip() + "\n"

    entry_body = visit(entry, True)
    if not module_bodies:
        return entry_body
    lines = entry_body.splitlines()
    insert_at = 1
    while insert_at < len(lines) and (
        not lines[insert_at].strip()
        or lines[insert_at].lstrip().startswith("#")
        or lines[insert_at].strip().startswith("profile ")
    ):
        insert_at += 1
    combined = [*lines[:insert_at], ""]
    for module in module_bodies:
        combined.extend(module.strip().splitlines())
        combined.append("")
    combined.extend(lines[insert_at:])
    return "\n".join(combined).rstrip() + "\n"

