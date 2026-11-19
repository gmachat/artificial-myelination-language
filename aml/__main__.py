"""Command-line interface for the AML prototype."""

from __future__ import annotations

import argparse
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

from . import __version__
from .backends import BackendError, check_backend, run_backend, target_catalog, write_backend_result
from .benchmark import run_benchmarks
from .compiler import CompileError, compile_source
from .module_loader import load_program
from .project import add_dependency, init_project, lock_project
from .services import diagnostic_for_error, diagnostic_json, format_source, compact_reference, reference_json


def _parser() -> argparse.ArgumentParser:
    parser = argparse.ArgumentParser(prog="aml", description="AML compiler")
    parser.add_argument("--version", action="version", version=f"AML {__version__}")
    commands = parser.add_subparsers(dest="command", required=True)
    benchmark = commands.add_parser(
        "benchmark", help="compare AML and Python reference source sizes"
    )
    benchmark.add_argument(
        "--json", action="store_true", help="emit machine-readable results"
    )
    benchmark.add_argument(
        "--language",
        choices=("all", "python", "javascript", "ruby", "go"),
        default="all",
        help="select the comparison language",
    )
    targets = ("python", "javascript", "js", "ruby", "go", "rust", "c", "java", "csharp", "cs", "swift", "react", "web", "fullstack", "ir")
    check = commands.add_parser("check")
    check.add_argument("file", type=Path)
    check.add_argument("--target", choices=targets, default="python")
    check.add_argument("--json", action="store_true", help="emit machine-readable diagnostics")
    run = commands.add_parser("run")
    run.add_argument("file", type=Path)
    run.add_argument("--target", choices=targets, default="python")
    build = commands.add_parser("build")
    build.add_argument("file", type=Path)
    build.add_argument("-o", "--output", type=Path)
    build_mode = build.add_mutually_exclusive_group()
    build_mode.add_argument("--target", choices=targets, default="python")
    build_mode.add_argument("--backend", type=Path, help="external aml-backend-1 executable")
    build.add_argument(
        "--force", action="store_true", help="write into a non-empty web output directory"
    )
    formatter = commands.add_parser("format", help="apply canonical AML formatting")
    formatter.add_argument("files", nargs="+", type=Path)
    formatter.add_argument("--check", action="store_true", help="check without writing")
    reference = commands.add_parser("reference", help="print the compact AI language reference")
    reference.add_argument("--json", action="store_true")
    commands.add_parser("lsp", help="start the AML language server over stdio")
    init = commands.add_parser("init", help="create an AML package")
    init.add_argument("directory", nargs="?", type=Path, default=Path("."))
    init.add_argument("--name")
    add = commands.add_parser("add", help="add a local AML package dependency")
    add.add_argument("name")
    add.add_argument("path", type=Path)
    add.add_argument("--project", type=Path, default=Path("."))
    lock = commands.add_parser("lock", help="create a deterministic AML lockfile")
    lock.add_argument("--project", type=Path, default=Path("."))
    targets_command = commands.add_parser("targets", help="list targets and dependencies")
    targets_command.add_argument("--json", action="store_true")
    backend_check = commands.add_parser("backend-check", help="validate an external backend")
    backend_check.add_argument("executable", type=Path)
    backend_check.add_argument("--json", action="store_true")
    return parser


def _read(path: Path) -> str:
    try:
        return path.read_text(encoding="utf-8")
    except OSError as error:
        raise SystemExit(f"aml: cannot read {path}: {error}") from error


def _compile(path: Path, target: str = "python") -> str:
    try:
        if target in {"python", "javascript", "js", "ruby", "go", "rust", "c", "java", "csharp", "cs", "swift", "ir"}:
            source = load_program(path)
        else:
            source = _read(path)
        return compile_source(source, str(path), target=target)
    except (CompileError, OSError) as error:
        raise SystemExit(f"{path}:{error}") from error


def main() -> int:
    args = _parser().parse_args()
    if args.command == "benchmark":
        print(run_benchmarks(as_json=args.json, language=args.language))
        return 0

    if args.command == "targets":
        catalog = target_catalog()
        if args.json:
            print(json.dumps(catalog, indent=2, sort_keys=True))
        else:
            for name, details in catalog.items():
                packages = ",".join(details["packages"]) or "none"
                platform = ",".join(details["platform"]) or "none"
                print(f"{name}: packages={packages}; platform={platform}")
        return 0

    if args.command == "backend-check":
        try:
            report = check_backend(args.executable.resolve())
        except BackendError as error:
            raise SystemExit(f"aml: {error}") from error
        if args.json:
            print(json.dumps(report, indent=2, sort_keys=True))
        else:
            print(f"ok: {args.executable} ({len(report['cases'])} protocol fixtures)")
        return 0

    if args.command == "format":
        changed = False
        for path in args.files:
            source = _read(path)
            formatted = format_source(source)
            if source != formatted:
                changed = True
                if not args.check:
                    path.write_text(formatted, encoding="utf-8")
                    print(path)
        if args.check and changed:
            return 1
        return 0

    if args.command == "reference":
        print(reference_json() if args.json else compact_reference(), end="")
        return 0

    if args.command == "lsp":
        from .lsp import serve

        serve()
        return 0

    if args.command == "init":
        try:
            for path in init_project(args.directory, args.name):
                print(path)
        except (OSError, ValueError) as error:
            raise SystemExit(f"aml: {error}") from error
        return 0

    if args.command == "add":
        try:
            print(add_dependency(args.project, args.name, args.path))
            print(lock_project(args.project))
        except (OSError, ValueError, KeyError, json.JSONDecodeError) as error:
            raise SystemExit(f"aml: {error}") from error
        return 0

    if args.command == "lock":
        try:
            print(lock_project(args.project))
        except (OSError, ValueError, KeyError, json.JSONDecodeError) as error:
            raise SystemExit(f"aml: {error}") from error
        return 0

    target_name = getattr(args, "target", "python")

    if args.command == "build" and args.backend is not None:
        from .ir import program_ir

        output = args.output or Path(f"{args.file.stem}-backend")
        if output.exists() and not output.is_dir():
            raise SystemExit(f"aml: backend output exists and is not a directory: {output}")
        if output.exists() and any(output.iterdir()) and not args.force:
            raise SystemExit(f"aml: backend output directory is not empty: {output}; use --force to update it")
        try:
            source = load_program(args.file)
            result = run_backend(args.backend.resolve(), program_ir(source, str(args.file)), str(args.file))
            write_backend_result(result, output)
        except (BackendError, CompileError, OSError) as error:
            raise SystemExit(f"aml: {error}") from error
        print(output)
        return 0
    if args.command == "check" and target_name == "fullstack":
        from .fullstack import parse_fullstack

        try:
            parse_fullstack(_read(args.file))
        except CompileError as error:
            if args.json:
                print(diagnostic_json([diagnostic_for_error(args.file, error)]))
                return 1
            raise SystemExit(f"{args.file}:{error}") from error
        print(diagnostic_json([]) if args.json else f"ok: {args.file}")
        return 0

    if args.command == "build" and target_name in {"web", "fullstack"}:
        source = _read(args.file)
        try:
            if target_name == "web":
                from .web import web_project_files

                files = web_project_files(source, str(args.file))
            else:
                from .fullstack import fullstack_project_files

                files = fullstack_project_files(source)
        except CompileError as error:
            raise SystemExit(f"{args.file}:{error}") from error
        output = args.output or Path(f"{args.file.stem}-{target_name}")
        if output.exists() and not output.is_dir():
            raise SystemExit(f"aml: web output exists and is not a directory: {output}")
        if output.exists() and any(output.iterdir()) and not args.force:
            raise SystemExit(
                f"aml: web output directory is not empty: {output}; use --force to update it"
            )
        for relative, content in files.items():
            destination = output / relative
            destination.parent.mkdir(parents=True, exist_ok=True)
            destination.write_text(content, encoding="utf-8")
        print(output)
        return 0

    if args.command == "check" and args.json:
        try:
            _compile(args.file, target=target_name)
            print(diagnostic_json([]))
            return 0
        except SystemExit as error:
            cause = error.__cause__ or error
            print(diagnostic_json([diagnostic_for_error(args.file, cause)]))
            return 1

    generated = _compile(args.file, target=target_name)

    if args.command == "check":
        print(f"ok: {args.file}")
        return 0
    if args.command == "build":
        if target_name == "react":
            suffix = ".jsx"
        elif target_name == "ir":
            suffix = ".amlir.json"
        elif target_name == "ruby":
            suffix = ".rb"
        elif target_name == "go":
            suffix = ".go"
        elif target_name == "rust":
            suffix = ".rs"
        elif target_name == "c":
            suffix = ".c"
        elif target_name == "java":
            suffix = ".java"
        elif target_name in {"csharp", "cs"}:
            suffix = ".cs"
        elif target_name == "swift":
            suffix = ".swift"
        else:
            suffix = ".js" if target_name in {"javascript", "js"} else ".py"
        output = args.output or args.file.with_suffix(suffix)
        output.parent.mkdir(parents=True, exist_ok=True)
        output.write_text(generated, encoding="utf-8")
        print(output)
        return 0

    if target_name in {"react", "web", "fullstack", "ir"}:
        raise SystemExit(
            "aml: browser output must be built before it can run; "
            f"use 'aml build <file> --target {target_name}'"
        )

    with tempfile.TemporaryDirectory(prefix="aml-") as directory:
        javascript = target_name in {"javascript", "js"}
        ruby = target_name == "ruby"
        go = target_name == "go"
        native_suffixes = {"rust": ".rs", "c": ".c", "java": ".java", "csharp": ".cs", "cs": ".cs", "swift": ".swift"}
        suffix = native_suffixes.get(target_name)
        target = Path(directory) / ("program.js" if javascript else "program.rb" if ruby else "main.go" if go else f"program{suffix}" if suffix else "program.py")
        target.write_text(generated, encoding="utf-8")
        if target_name == "python" and getattr(sys, "frozen", False):
            namespace = {"__name__": "__main__", "__file__": str(args.file)}
            try:
                exec(compile(generated, str(args.file), "exec"), namespace)
            except SystemExit as error:
                return int(error.code) if isinstance(error.code, int) else 1
            return 0
        if target_name in {"c", "rust"}:
            executable = Path(directory) / "program"
            compiler = "cc" if target_name == "c" else "rustc"
            compiled = subprocess.run([compiler, str(target), "-o", str(executable)], check=False)
            if compiled.returncode:
                return compiled.returncode
            command = [str(executable)]
        elif target_name == "swift":
            command = ["swift", str(target)]
        elif target_name == "java":
            command = ["java", str(target)]
        elif target_name in {"csharp", "cs"}:
            raise SystemExit("aml: C# output must be built with a C# compiler or .NET SDK")
        else:
            command = ["node", str(target)] if javascript else ["ruby", str(target)] if ruby else ["go", "run", str(target)] if go else [sys.executable, str(target)]
        try:
            environment = None
            if go:
                environment = dict(os.environ)
                environment["GOCACHE"] = str(Path(directory) / "go-cache")
            result = subprocess.run(command, check=False, env=environment)
        except FileNotFoundError as error:
            platform = {"go": "Go", "ruby": "Ruby", "javascript": "Node.js", "js": "Node.js", "c": "a C compiler", "rust": "Rust", "swift": "Swift", "java": "Java"}.get(target_name, target_name)
            raise SystemExit(f"aml: {platform} is required to run {target_name} output") from error
        return result.returncode


if __name__ == "__main__":
    raise SystemExit(main())

