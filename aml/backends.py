"""Language-neutral subprocess protocol for external AML backends."""

from __future__ import annotations

from dataclasses import dataclass
import json
from pathlib import Path, PurePosixPath
import subprocess
from typing import Any


PROTOCOL = "aml-backend-1"


class BackendError(ValueError):
    pass


@dataclass(frozen=True)
class Artifact:
    path: str
    content: str
    executable: bool = False


@dataclass(frozen=True)
class BackendResult:
    artifacts: tuple[Artifact, ...]
    entry: str | None = None
    diagnostics: tuple[dict[str, Any], ...] = ()


def backend_request(ir: dict[str, Any], source: str) -> str:
    return json.dumps(
        {"protocol": PROTOCOL, "source": source, "ir": ir},
        sort_keys=True,
        separators=(",", ":"),
    )


def _safe_path(value: object) -> str:
    if not isinstance(value, str) or not value:
        raise BackendError("artifact path must be a non-empty string")
    path = PurePosixPath(value)
    if path.is_absolute() or ".." in path.parts or value.startswith(("/", "\\")):
        raise BackendError(f"unsafe artifact path: {value!r}")
    return path.as_posix()


def parse_backend_response(source: str) -> BackendResult:
    try:
        data = json.loads(source)
    except json.JSONDecodeError as error:
        raise BackendError(f"backend returned invalid JSON: {error}") from error
    if not isinstance(data, dict) or data.get("protocol") != PROTOCOL:
        raise BackendError(f"backend response must use protocol {PROTOCOL!r}")
    raw_artifacts = data.get("artifacts")
    if not isinstance(raw_artifacts, list) or not raw_artifacts:
        raise BackendError("backend response needs at least one artifact")
    artifacts: list[Artifact] = []
    seen: set[str] = set()
    for raw in raw_artifacts:
        if not isinstance(raw, dict):
            raise BackendError("each artifact must be an object")
        path = _safe_path(raw.get("path"))
        content = raw.get("content")
        if not isinstance(content, str):
            raise BackendError(f"artifact {path!r} content must be a string")
        if path in seen:
            raise BackendError(f"duplicate artifact path: {path!r}")
        seen.add(path)
        executable = raw.get("executable", False)
        if not isinstance(executable, bool):
            raise BackendError(f"artifact {path!r} executable must be boolean")
        artifacts.append(Artifact(path, content, executable))
    raw_entry = data.get("entry")
    entry = _safe_path(raw_entry) if raw_entry is not None else None
    if entry is not None and entry not in seen:
        raise BackendError(f"entry {entry!r} is not an artifact")
    diagnostics = data.get("diagnostics", [])
    if not isinstance(diagnostics, list) or any(not isinstance(item, dict) for item in diagnostics):
        raise BackendError("diagnostics must be a list of objects")
    return BackendResult(tuple(artifacts), entry, tuple(diagnostics))


def run_backend(
    executable: Path, ir: dict[str, Any], source: str, timeout: float = 30.0
) -> BackendResult:
    """Invoke an external backend without a shell or language-specific loader."""
    try:
        completed = subprocess.run(
            [str(executable)],
            input=backend_request(ir, source),
            text=True,
            capture_output=True,
            check=False,
            timeout=timeout,
        )
    except subprocess.TimeoutExpired as error:
        raise BackendError(f"backend timed out after {timeout:g} seconds") from error
    except OSError as error:
        raise BackendError(f"cannot run backend {executable}: {error}") from error
    if completed.returncode:
        detail = completed.stderr.strip() or f"exit status {completed.returncode}"
        raise BackendError(f"backend failed: {detail}")
    return parse_backend_response(completed.stdout)


def check_backend(executable: Path) -> dict[str, object]:
    """Check protocol validity and deterministic output on representative IR."""
    from .ir import program_ir

    fixtures = {
        "scalar": """app scalar
profile script
task double(value: Int) -> Int:
  return value * 2
task main:
  show double(21)
""",
        "structured": """app structured
profile script
record Item:
  name: Text
  values: List[Int]
  note: Optional[Text]
task first(item: Item) -> Int:
  return item.values[0]
task main:
  show first({name: "sample", values: [42], note: none})
""",
        "contracts": """app contracts
profile script
task classify(value: Int) -> Text:
  require value >= 0
  ensure result != ""
  if value > 0:
    return "positive"
  else:
    return "zero"
task main:
  show classify(1)
""",
    }
    cases: list[dict[str, object]] = []
    for name, source in fixtures.items():
        ir = program_ir(source, f"{name}.aml")
        first = run_backend(executable, ir, f"{name}.aml")
        second = run_backend(executable, ir, f"{name}.aml")
        if first != second:
            raise BackendError(f"backend output is not deterministic for fixture {name!r}")
        cases.append(
            {
                "name": name,
                "artifacts": len(first.artifacts),
                "entry": first.entry,
                "valid": True,
            }
        )
    return {"protocol": PROTOCOL, "backend": str(executable), "valid": True, "cases": cases}


def write_backend_result(result: BackendResult, directory: Path) -> None:
    directory.mkdir(parents=True, exist_ok=True)
    root = directory.resolve()
    for artifact in result.artifacts:
        destination = (directory / artifact.path).resolve()
        if destination != root and root not in destination.parents:
            raise BackendError(f"artifact escapes output directory: {artifact.path!r}")
        destination.parent.mkdir(parents=True, exist_ok=True)
        destination.write_text(artifact.content, encoding="utf-8")
        if artifact.executable:
            destination.chmod(destination.stat().st_mode | 0o111)


def target_catalog() -> dict[str, dict[str, object]]:
    return {
        "python": {"kind": "core", "output": "file", "platform": ["Python 3"], "packages": []},
        "javascript": {"kind": "core", "output": "file", "platform": ["JavaScript runtime"], "packages": []},
        "ruby": {"kind": "core", "output": "file", "platform": ["Ruby"], "packages": []},
        "go": {"kind": "core", "output": "file", "platform": ["Go toolchain"], "packages": []},
        "rust": {"kind": "core", "output": "file", "platform": ["Rust toolchain"], "packages": []},
        "c": {"kind": "core", "output": "file", "platform": ["C11 compiler"], "packages": []},
        "java": {"kind": "structured-source", "output": "file", "platform": ["Java 16+ toolchain"], "packages": []},
        "csharp": {"kind": "structured-source", "output": "file", "platform": [".NET 5+ SDK"], "packages": []},
        "swift": {"kind": "structured-source", "output": "file", "platform": ["Swift toolchain"], "packages": []},
        "react": {"kind": "framework", "output": "file", "platform": ["JavaScript runtime"], "packages": ["react"]},
        "web": {"kind": "framework", "output": "project", "platform": ["Node.js"], "packages": ["react", "react-dom", "vite"]},
        "fullstack": {"kind": "framework", "output": "project", "platform": ["Node.js"], "packages": ["express", "react", "react-dom", "vite"]},
        "ir": {"kind": "portable", "output": "file", "platform": [], "packages": []},
        "external": {"kind": "protocol", "output": "artifacts", "platform": ["backend executable"], "packages": []},
    }
