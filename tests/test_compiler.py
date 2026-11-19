import subprocess
import sys
import tempfile
from pathlib import Path
import unittest

from aml import CompileError, compile_source


class CompilerTests(unittest.TestCase):
    def test_compiles_and_runs_program(self):
        source = """app hello
profile script

task greet(name: Text) -> Text:
  return "Hello, " + name

task main:
  show greet("AML")
"""
        generated = compile_source(source)
        with tempfile.TemporaryDirectory() as directory:
            program = Path(directory) / "program.py"
            program.write_text(generated, encoding="utf-8")
            result = subprocess.run(
                [sys.executable, str(program)],
                check=True,
                capture_output=True,
                text=True,
            )
        self.assertEqual(result.stdout, "Hello, AML\n")

    def test_compiles_conditionals_and_literals(self):
        generated = compile_source("""app flags
profile script
task choose(flag: Bool) -> Text:
  if flag == true:
    return "yes"
  else:
    return "no"
""")
        self.assertIn("if flag == True:", generated)
        self.assertIn("else:", generated)

    def test_rejects_unknown_type(self):
        with self.assertRaisesRegex(CompileError, "unknown type 'String'"):
            compile_source("""app bad
profile script
task greet(name: String):
  show name
""")

    def test_rejects_tabs(self):
        with self.assertRaisesRegex(CompileError, "tabs are not allowed"):
            compile_source("app bad\nprofile script\ntask main:\n\tshow \"no\"\n")

    def test_requires_app_first(self):
        with self.assertRaisesRegex(CompileError, "first declaration"):
            compile_source("task main:\n  show \"no\"\n")

    def test_contracts_run_and_are_exposed_as_metadata(self):
        generated = compile_source("""app scores
profile script
task normalize(score: Int) -> Int:
  require score >= 0
  ensure result <= 100
  return score
""")
        namespace = {"__name__": "test_module"}
        exec(generated, namespace)
        self.assertEqual(namespace["normalize"](80), 80)
        self.assertEqual(
            namespace["__aml_contracts__"]["normalize"]["ensure"],
            ["result <= 100"],
        )
        with self.assertRaisesRegex(Exception, "require failed"):
            namespace["normalize"](-1)
        with self.assertRaisesRegex(Exception, "ensure failed"):
            namespace["normalize"](120)

    def test_profile_validates_argument_types(self):
        generated = compile_source("""app typed
profile script
task double(value: Int) -> Int:
  return value * 2
""")
        namespace = {"__name__": "test_module"}
        exec(generated, namespace)
        with self.assertRaisesRegex(Exception, "expected int, got str"):
            namespace["double"]("2")

    def test_semantic_checker_rejects_wrong_return_type(self):
        with self.assertRaisesRegex(CompileError, "returns Text, expected Int"):
            compile_source("""app typed
profile script
task wrong() -> Int:
  return "not an integer"
""")

    def test_typed_task_requires_a_return(self):
        with self.assertRaisesRegex(CompileError, "declares a return type but has no return"):
            compile_source("""app typed
profile script
task missing() -> Int:
  show "forgot"
""")

    def test_requires_known_profile(self):
        with self.assertRaisesRegex(CompileError, "unknown profile"):
            compile_source("""app bad
profile magical
task main:
  show "no"
""")


if __name__ == "__main__":
    unittest.main()

