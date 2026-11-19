"""Small, deterministic local package manifests and lockfiles."""

from __future__ import annotations

import hashlib
import json
from pathlib import Path
import re


MANIFEST = "aml.package.json"
LOCKFILE = "aml.lock.json"


def _name(value: str) -> str:
    normalized = re.sub(r"[^a-z0-9_]+", "_", value.lower()).strip("_")
    return normalized or "aml_project"


def init_project(directory: Path, name: str | None = None) -> list[Path]:
    directory.mkdir(parents=True, exist_ok=True)
    manifest = directory / MANIFEST
    entry = directory / "main.aml"
    if manifest.exists():
        raise FileExistsError(f"manifest already exists: {manifest}")
    package_name = _name(name or directory.name)
    manifest.write_text(
        json.dumps(
            {
                "name": package_name,
                "version": "0.1.0",
                "entry": "main.aml",
                "dependencies": {},
            },
            indent=2,
        )
        + "\n",
        encoding="utf-8",
    )
    created = [manifest]
    if not entry.exists():
        entry.write_text(
            f'app {package_name}\nprofile script\n\ntask main:\n  show "Hello from AML"\n',
            encoding="utf-8",
        )
        created.append(entry)
    return created


def add_dependency(directory: Path, name: str, dependency_path: Path) -> Path:
    manifest_path = directory / MANIFEST
    manifest = json.loads(manifest_path.read_text(encoding="utf-8"))
    resolved = dependency_path.resolve()
    dependency_manifest = resolved / MANIFEST
    if not dependency_manifest.is_file():
        raise FileNotFoundError(f"dependency has no {MANIFEST}: {resolved}")
    relative = Path(resolved).relative_to(directory.resolve()) if resolved.is_relative_to(directory.resolve()) else resolved
    manifest.setdefault("dependencies", {})[_name(name)] = {"path": str(relative)}
    manifest_path.write_text(json.dumps(manifest, indent=2, sort_keys=True) + "\n")
    return manifest_path


def _digest(directory: Path) -> str:
    digest = hashlib.sha256()
    for path in sorted(directory.rglob("*.aml")):
        digest.update(path.relative_to(directory).as_posix().encode())
        digest.update(b"\0")
        digest.update(path.read_bytes())
        digest.update(b"\0")
    digest.update((directory / MANIFEST).read_bytes())
    return digest.hexdigest()


def lock_project(directory: Path) -> Path:
    directory = directory.resolve()
    manifest = json.loads((directory / MANIFEST).read_text(encoding="utf-8"))
    locked: dict[str, object] = {}
    for name, declaration in sorted(manifest.get("dependencies", {}).items()):
        raw_path = Path(declaration["path"])
        dependency = raw_path if raw_path.is_absolute() else directory / raw_path
        dependency = dependency.resolve()
        dependency_manifest = json.loads((dependency / MANIFEST).read_text(encoding="utf-8"))
        locked[name] = {
            "path": str(dependency),
            "name": dependency_manifest["name"],
            "version": dependency_manifest["version"],
            "sha256": _digest(dependency),
        }
    lock = {
        "lock_version": 1,
        "package": manifest["name"],
        "dependencies": locked,
    }
    target = directory / LOCKFILE
    target.write_text(json.dumps(lock, indent=2, sort_keys=True) + "\n")
    return target

