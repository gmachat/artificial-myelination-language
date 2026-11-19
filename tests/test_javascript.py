import shutil
import subprocess
import tempfile
from pathlib import Path
import unittest

from aml import compile_source


class JavaScriptCompilerTests(unittest.TestCase):
    def test_generates_javascript_with_types_and_contracts(self):
        generated = compile_source(
            """app scores
profile script
task classify(score: Int) -> Text:
  require score >= 0 and score <= 100
  ensure result != ""
  if score >= 80:
    return "ready"
  else:
    return "keep learning"
task main:
  show classify(92)
""",
            target="javascript",
        )
        self.assertIn("function classify(score) {", generated)
        self.assertIn('__amlExpect("classify.score", score, "Int")', generated)
        self.assertIn("score >= 0 && score <= 100", generated)
        self.assertIn('result !== ""', generated)

    @unittest.skipUnless(shutil.which("node"), "Node.js is not installed")
    def test_generated_javascript_runs_with_node(self):
        generated = compile_source(
            """app hello
profile script
task greet(name: Text) -> Text:
  return "Hello, " + name + "!"
task main:
  show greet("JavaScript")
""",
            target="javascript",
        )
        with tempfile.TemporaryDirectory() as directory:
            program = Path(directory) / "program.js"
            program.write_text(generated, encoding="utf-8")
            result = subprocess.run(
                ["node", str(program)], capture_output=True, text=True, check=True
            )
        self.assertEqual(result.stdout, "Hello, JavaScript!\n")

    @unittest.skipUnless(shutil.which("node"), "Node.js is not installed")
    def test_contract_failure_has_structured_javascript_error(self):
        generated = compile_source(
            """app invalid
profile script
task positive(value: Int) -> Int:
  require value > 0
  return value
task main:
  show positive(-1)
""",
            target="js",
        )
        with tempfile.TemporaryDirectory() as directory:
            program = Path(directory) / "program.js"
            program.write_text(generated, encoding="utf-8")
            result = subprocess.run(
                ["node", str(program)], capture_output=True, text=True, check=False
            )
        self.assertEqual(result.returncode, 1)
        self.assertIn(
            "AML_ERROR: contract | positive require failed: value > 0",
            result.stderr,
        )


if __name__ == "__main__":
    unittest.main()

