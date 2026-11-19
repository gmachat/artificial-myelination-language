import io
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

from aml import compile_source
from aml.backends import BackendError, check_backend, parse_backend_response, run_backend
from aml.benchmark import _counter
from aml.lsp import _diagnostics, _read_message, _write_message
from aml.ir import program_ir
from aml.module_loader import load_program
from aml.project import add_dependency, init_project, lock_project
from aml.services import compact_reference, format_source


class LanguageServiceTests(unittest.TestCase):
    def test_formatter_is_canonical_and_idempotent(self):
        source = 'app demo  \n\n\nprofile script\n\ttask main:\n\t\tshow "hi"   \n'
        formatted = format_source(source)
        self.assertEqual(
            formatted,
            'app demo\n\nprofile script\n  task main:\n    show "hi"\n',
        )
        self.assertEqual(format_source(formatted), formatted)

    def test_compact_ai_reference_stays_under_budget(self):
        method, count = _counter()
        self.assertIn(method, {"cl100k_base", "lexical_estimate"})
        self.assertLessEqual(count(compact_reference()), 500)

    def test_json_diagnostics_have_precise_location(self):
        diagnostics = _diagnostics("file:///tmp/bad.aml", "app bad\nprofile script\ntask main:\n\tshow 1\n")
        self.assertEqual(len(diagnostics), 1)
        self.assertEqual(diagnostics[0]["range"]["start"]["line"], 3)
        self.assertEqual(diagnostics[0]["source"], "aml")

    def test_lsp_framing_round_trip(self):
        stream = io.BytesIO()
        payload = {"jsonrpc": "2.0", "id": 1, "method": "initialize"}
        _write_message(stream, payload)
        stream.seek(0)
        self.assertEqual(_read_message(stream), payload)


class ModuleAndPackageTests(unittest.TestCase):
    def test_local_module_runs_identically_across_core_backends(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / "math.aml").write_text(
                """module math
task double(value: Int) -> Int:
  return value * 2
"""
            )
            entry = root / "main.aml"
            entry.write_text(
                """app modules
profile script
use "math.aml"
task main:
  show double(21)
"""
            )
            source = load_program(entry)
            self.assertNotIn("use ", source)
            outputs = []
            for target, command, suffix in (
                ("python", [sys.executable], ".py"),
                ("javascript", ["node"], ".js"),
                ("ruby", ["ruby"], ".rb"),
                ("go", ["go", "run"], ".go"),
            ):
                generated = compile_source(source, str(entry), target=target)
                program = root / f"program{suffix}"
                program.write_text(generated)
                result = subprocess.run(
                    [*command, str(program)], capture_output=True, text=True, check=True,
                    env={**os.environ, "GOCACHE": str(root / "go-cache")} if target == "go" else None,
                )
                outputs.append(result.stdout)
            self.assertEqual(outputs, ["42\n", "42\n", "42\n", "42\n"])

    def test_local_package_lock_is_deterministic(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            dependency = root / "dependency"
            application = root / "application"
            init_project(dependency, "utility")
            init_project(application, "app")
            add_dependency(application, "utility", dependency)
            first = lock_project(application).read_text()
            second = lock_project(application).read_text()
            self.assertEqual(first, second)
            locked = json.loads(first)
            self.assertEqual(locked["dependencies"]["utility"]["version"], "0.1.0")
            self.assertEqual(len(locked["dependencies"]["utility"]["sha256"]), 64)


class StructuredTypeTests(unittest.TestCase):
    SOURCE = """app typed
profile script
record Person:
  name: Text
  scores: List[Int]
  nickname: Optional[Text]
task first(person: Person) -> Int:
  return person.scores[0]
task main:
  show first({name: "Ada", scores: [42, 7], nickname: none})
"""

    def test_records_lists_and_optionals_match_across_backends(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            outputs = []
            for target, command, suffix in (
                ("python", [sys.executable], ".py"),
                ("javascript", ["node"], ".js"),
                ("ruby", ["ruby"], ".rb"),
                ("go", ["go", "run"], ".go"),
            ):
                program = root / f"typed{suffix}"
                program.write_text(compile_source(self.SOURCE, target=target))
                result = subprocess.run(
                    [*command, str(program)], capture_output=True, text=True, check=True,
                    env={**os.environ, "GOCACHE": str(root / "go-cache")} if target == "go" else None,
                )
                outputs.append(result.stdout)
            self.assertEqual(outputs, ["42\n", "42\n", "42\n", "42\n"])

    def test_nested_type_errors_are_caught_before_generation(self):
        bad = self.SOURCE.replace("scores: [42, 7]", 'scores: [42, "bad"]')
        for target in ("python", "javascript", "ruby", "go"):
            with self.subTest(target=target):
                with self.assertRaisesRegex(Exception, "list mixes Int and Text"):
                    compile_source(bad, target=target)

    def test_portable_ir_contains_target_neutral_structure(self):
        ir = program_ir(self.SOURCE, "structured.aml")
        self.assertEqual(ir["aml_ir"], "1.1")
        self.assertEqual(ir["semantic"], "typed")
        self.assertEqual(ir["records"][0]["fields"][1]["type"], "List[Int]")
        returned = ir["tasks"][0]["body"][0]["value"]
        self.assertEqual(returned["kind"], "index")
        self.assertEqual(returned["type"], "Int")
        self.assertEqual(returned["value"]["kind"], "member")
        self.assertEqual(returned["value"]["type"], "List[Int]")

    def test_ir_backends_check_ensure_for_nested_returns(self):
        source = """app contracts
profile script
task choose(value: Int) -> Text:
  ensure result == "impossible"
  if value > 0:
    return "yes"
  else:
    return "no"
task main:
  show choose(1)
"""
        for target, command, suffix in (
            ("ruby", ["ruby"], ".rb"),
            ("go", ["go", "run"], ".go"),
        ):
            with self.subTest(target=target), tempfile.TemporaryDirectory() as directory:
                program = Path(directory) / f"contracts{suffix}"
                program.write_text(compile_source(source, target=target))
                result = subprocess.run(
                    [*command, str(program)], capture_output=True, text=True,
                    env={**os.environ, "GOCACHE": str(Path(directory) / "go-cache")} if target == "go" else None,
                )
                self.assertNotEqual(result.returncode, 0)
                self.assertIn("ensure failed", result.stderr)


class ExternalBackendTests(unittest.TestCase):
    def test_language_neutral_backend_protocol(self):
        with tempfile.TemporaryDirectory() as directory:
            backend = Path(directory) / "backend"
            backend.write_text(
                """#!/usr/bin/env python3
import json, sys
request = json.load(sys.stdin)
app = request["ir"]["app"]
json.dump({
  "protocol": "aml-backend-1",
  "entry": "main.txt",
  "artifacts": [{"path": "main.txt", "content": "app=" + app + "\\n"}]
}, sys.stdout)
"""
            )
            backend.chmod(0o755)
            ir = program_ir("app demo\nprofile script\ntask main:\n  show 1\n")
            result = run_backend(backend, ir, "demo.aml")
            self.assertEqual(result.entry, "main.txt")
            self.assertEqual(result.artifacts[0].content, "app=demo\n")

            report = check_backend(backend)
            self.assertTrue(report["valid"])
            self.assertEqual(len(report["cases"]), 3)

    def test_backend_cannot_escape_output_directory(self):
        response = json.dumps({
            "protocol": "aml-backend-1",
            "artifacts": [{"path": "../outside.txt", "content": "bad"}],
        })
        with self.assertRaisesRegex(BackendError, "unsafe artifact path"):
            parse_backend_response(response)


if __name__ == "__main__":
    unittest.main()

