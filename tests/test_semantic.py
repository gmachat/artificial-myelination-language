import unittest

from aml import CompileError
from aml.ir import program_ir


def program(body: str) -> str:
    return "app semantic\nprofile script\n" + body


class SemanticTests(unittest.TestCase):
    def test_unknown_name_is_rejected(self):
        with self.assertRaisesRegex(CompileError, "unknown name 'missing'"):
            program_ir(program("task main:\n  show missing\n"))

    def test_call_arity_and_argument_types_are_checked(self):
        source = program(
            "task greet(name: Text) -> Text:\n"
            "  return name\n"
            "task main:\n"
            "  show greet(42)\n"
        )
        with self.assertRaisesRegex(CompileError, "expects Text, got Int"):
            program_ir(source)

    def test_conditions_and_contracts_require_bool(self):
        with self.assertRaisesRegex(CompileError, "if expects Bool, got Int"):
            program_ir(program("task main:\n  if 1:\n    show 1\n"))
        with self.assertRaisesRegex(CompileError, "require expects Bool, got Text"):
            program_ir(program('task main:\n  require "yes"\n'))

    def test_record_fields_lists_and_optionals_are_typed(self):
        source = program(
            "record Person:\n"
            "  name: Text\n"
            "  scores: List[Int]\n"
            "  note: Optional[Text]\n"
            "task score(person: Person) -> Int:\n"
            "  return person.scores[0]\n"
            "task main:\n"
            '  show score({name: "Ada", scores: [42], note: none})\n'
        )
        ir = program_ir(source)
        call = ir["tasks"][1]["body"][0]["expression"]
        self.assertEqual(call["type"], "Int")
        record = call["arguments"][0]
        self.assertEqual(record["type"], "Person")
        self.assertEqual(record["fields"][1]["value"]["type"], "List[Int]")
        self.assertEqual(record["fields"][2]["value"]["type"], "None")

    def test_wrong_return_and_missing_record_field_are_rejected(self):
        with self.assertRaisesRegex(CompileError, "returns Text, expected Int"):
            program_ir(program('task wrong() -> Int:\n  return "no"\n'))
        source = program(
            "record Item:\n"
            "  name: Text\n"
            "  count: Int\n"
            "task use(item: Item) -> Text:\n"
            "  return item.name\n"
            "task main:\n"
            '  show use({name: "x"})\n'
        )
        with self.assertRaisesRegex(CompileError, "does not match a declared record"):
            program_ir(source)

    def test_duplicate_declarations_are_rejected(self):
        with self.assertRaisesRegex(CompileError, "duplicate parameter 'value'"):
            program_ir(program("task bad(value: Int, value: Int):\n  show value\n"))

    def test_void_task_is_allowed_as_statement_not_value(self):
        valid = program(
            "task announce():\n"
            '  show "hello"\n'
            "task main:\n"
            "  announce()\n"
        )
        ir = program_ir(valid)
        call = ir["tasks"][1]["body"][0]["expression"]
        self.assertEqual(call["type"], "Void")
        invalid = program(
            "task announce():\n"
            '  show "hello"\n'
            "task main:\n"
            "  let result = announce()\n"
        )
        with self.assertRaisesRegex(CompileError, "does not return a value"):
            program_ir(invalid)


if __name__ == "__main__":
    unittest.main()
