"""Build and smoke-test one platform-specific standalone AML executable."""

from __future__ import annotations

import hashlib
import os
from pathlib import Path
import platform
import shutil
import subprocess
import sys


ROOT = Path(__file__).resolve().parents[1]


def platform_name() -> str:
    systems = {"Darwin": "macos", "Linux": "linux", "Windows": "windows"}
    machines = {"AMD64": "x86_64", "x86_64": "x86_64", "arm64": "arm64", "aarch64": "arm64"}
    system = systems.get(platform.system(), platform.system().lower())
    machine = machines.get(platform.machine(), platform.machine().lower())
    return f"{system}-{machine}"


def main() -> int:
    sys.path.insert(0, str(ROOT))
    from aml import __version__

    release = ROOT / "release" / f"aml-{__version__}-{platform_name()}"
    work = ROOT / "build" / "pyinstaller" / platform_name()
    spec = work / "spec"
    release.mkdir(parents=True, exist_ok=True)
    spec.mkdir(parents=True, exist_ok=True)
    executable_name = "aml.exe" if platform.system() == "Windows" else "aml"
    command = [
        sys.executable,
        "-m",
        "PyInstaller",
        "--onefile",
        "--clean",
        "--noconfirm",
        "--name",
        "aml",
        "--distpath",
        str(release),
        "--workpath",
        str(work / "work"),
        "--specpath",
        str(spec),
        "--collect-submodules",
        "aml",
        "--collect-data",
        "aml",
        str(ROOT / "scripts" / "aml_entry.py"),
    ]
    environment = dict(os.environ)
    environment["PYINSTALLER_CONFIG_DIR"] = str(work / "cache")
    subprocess.run(command, cwd=ROOT, env=environment, check=True)
    executable = release / executable_name
    subprocess.run([str(executable), "--version"], cwd=ROOT, check=True)
    subprocess.run([str(executable), "check", str(ROOT / "examples" / "hello.aml")], cwd=ROOT, check=True)
    subprocess.run([str(executable), "run", str(ROOT / "examples" / "hello.aml")], cwd=ROOT, check=True)
    digest = hashlib.sha256(executable.read_bytes()).hexdigest()
    checksum = release / "SHA256SUMS"
    checksum.write_text(f"{digest}  {executable.name}\n", encoding="utf-8")
    archive_base = ROOT / "release" / f"aml-{__version__}-{platform_name()}"
    if platform.system() == "Windows":
        shutil.make_archive(str(archive_base), "zip", release.parent, release.name)
    else:
        shutil.make_archive(str(archive_base), "gztar", release.parent, release.name)
    print(executable)
    print(checksum)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
