from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest

from aml import CompileError, compile_source


PROGRAM = """app native
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
"""


class StaticBackendTests(unittest.TestCase):
    def test_c_and_rust_compile_and_run(self):
        targets = (
            ("c", "cc", ".c"),
            ("rust", "rustc", ".rs"),
        )
        for target, compiler, suffix in targets:
            if not shutil.which(compiler):
                continue
            with self.subTest(target=target), tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                source = root / f"program{suffix}"
                executable = root / "program"
                source.write_text(compile_source(PROGRAM, target=target))
                subprocess.run(
                    [compiler, str(source), "-o", str(executable)],
                    capture_output=True,
                    text=True,
                    check=True,
                )
                result = subprocess.run(
                    [str(executable)], capture_output=True, text=True, check=True
                )
                self.assertEqual(result.stdout, "ready\n")

    def test_swift_output_parses_when_toolchain_is_available(self):
        generated = compile_source(PROGRAM, target="swift")
        self.assertIn("func aml_classify", generated)
        if not shutil.which("swiftc"):
            return
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "program.swift"
            source.write_text(generated)
            subprocess.run(
                ["swiftc", "-frontend", "-parse", str(source)],
                capture_output=True,
                text=True,
                check=True,
            )

    def test_rust_structures_lists_and_optionals(self):
        if not shutil.which("rustc"):
            return
        source = """app rust_structured
profile script
record Person:
  name: Text
  names: List[Text]
  note: Optional[Text]
task label(person: Person) -> Text:
  if person.note != none:
    return person.name
  else:
    return person.names[0]
task main:
  show label({name: "Ada", names: ["A"], note: "ready"})
  show label({name: "Bea", names: ["B"], note: none})
"""
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            generated = root / "structured.rs"
            executable = root / "structured"
            generated.write_text(compile_source(source, target="rust"))
            subprocess.run(
                ["rustc", str(generated), "-o", str(executable)],
                capture_output=True,
                text=True,
                check=True,
            )
            result = subprocess.run(
                [str(executable)], capture_output=True, text=True, check=True
            )
            self.assertEqual(result.stdout, "Ada\nB\n")

    def test_java_and_csharp_have_native_contract_shapes(self):
        java = compile_source(PROGRAM, target="java")
        csharp = compile_source(PROGRAM, target="csharp")
        self.assertIn("class AMLProgram", java)
        self.assertIn("result.equals", java)
        self.assertIn("public static void main", java)
        self.assertIn("class AMLProgram", csharp)
        self.assertIn("throw new Exception", csharp)
        self.assertIn("public static void Main", csharp)

    def test_java_structured_source_uses_records_lists_and_optionals(self):
        source = """app java_structured
profile script
record Person:
  name: Text
  names: List[Text]
  note: Optional[Text]
task label(person: Person) -> Text:
  if person.note != none:
    return person.name
  else:
    return person.names[0]
task main:
  show label({name: "Ada", names: ["A"], note: "ready"})
"""
        generated = compile_source(source, target="java")
        self.assertIn("record Person(String name, List<String> names, Optional<String> note)", generated)
        self.assertIn('new Person("Ada", List.of("A"), Optional.of("ready"))', generated)
        self.assertIn("Objects.equals(person.note(), Optional.empty())", generated)
        self.assertIn("person.names().get((int) 0L)", generated)

    def test_csharp_structured_source_uses_records_lists_and_nullables(self):
        source = """app csharp_structured
profile script
record Person:
  name: Text
  names: List[Text]
  note: Optional[Text]
task label(person: Person) -> Text:
  if person.note != none:
    return person.name
  else:
    return person.names[0]
task main:
  show label({name: "Ada", names: ["A"], note: "ready"})
"""
        generated = compile_source(source, target="csharp")
        self.assertIn("record Person(string name, List<string> names, string? note)", generated)
        self.assertIn('new Person("Ada", new List<string> { "A" }, "ready")', generated)
        self.assertIn("person.note != null", generated)
        self.assertIn("person.names[(int) 0]", generated)

    def test_swift_structured_source_uses_structs_arrays_and_optionals(self):
        source = """app swift_structured
profile script
record Person:
  name: Text
  names: List[Text]
  note: Optional[Text]
task label(person: Person) -> Text:
  if person.note != none:
    return person.name
  else:
    return person.names[0]
task main:
  show label({name: "Ada", names: ["A"], note: "ready"})
"""
        generated = compile_source(source, target="swift")
        self.assertIn("struct Person: Equatable", generated)
        self.assertIn("let names: [String]", generated)
        self.assertIn("let note: String?", generated)
        self.assertIn('Person(name: "Ada", names: ["A"], note: "ready")', generated)
        self.assertIn("person.names[Int(Int64(0))]", generated)
        if shutil.which("swiftc"):
            with tempfile.TemporaryDirectory() as directory:
                path = Path(directory) / "structured.swift"
                path.write_text(generated)
                subprocess.run(
                    ["swiftc", "-frontend", "-parse", str(path)],
                    capture_output=True,
                    text=True,
                    check=True,
                )

    def test_c_structures_lists_and_optionals_compile_and_run(self):
        if not shutil.which("cc"):
            return
        source = """app c_structured
profile script
record Person:
  name: Text
  names: List[Text]
  note: Optional[Text]
task label(person: Person) -> Text:
  if person.note != none:
    return person.name
  else:
    return person.names[0]
task main:
  show label({name: "Ada", names: ["A"], note: "ready"})
  show label({name: "Bea", names: ["B"], note: none})
"""
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            generated = root / "structured.c"
            executable = root / "structured"
            generated.write_text(compile_source(source, target="c"))
            subprocess.run(
                ["cc", "-std=c11", str(generated), "-o", str(executable)],
                capture_output=True,
                text=True,
                check=True,
            )
            result = subprocess.run(
                [str(executable)], capture_output=True, text=True, check=True
            )
            self.assertEqual(result.stdout, "Ada\nB\n")

    def test_c_rejects_recursive_value_types_explicitly(self):
        source = """app recursive
profile script
record Node:
  next: Optional[Node]
task main:
  show "ok"
"""
        with self.assertRaisesRegex(CompileError, "recursive type"):
            compile_source(source, target="c")


if __name__ == "__main__":
    unittest.main()
