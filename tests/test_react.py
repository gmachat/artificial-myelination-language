import unittest

from aml import CompileError, compile_source


COUNTER = """app counter
profile react
component Counter(title: Text):
  state count: Int = 0
  state name: Text = ""
  require title != ""
  view:
    column:
      heading title
      text "Count: {count}"
      when count == 0:
        text "Ready to begin"
      else:
        text "Work in progress"
      button "Add one":
        on click:
          set count = count + 1
      input name
      text "Hello, {name}"
"""


class ReactCompilerTests(unittest.TestCase):
    def test_compiles_typed_component_state_and_events(self):
        generated = compile_source(COUNTER, target="react")
        self.assertIn('import React, { useState } from "react";', generated)
        self.assertIn("export function Counter({ title })", generated)
        self.assertIn("const [count, setCount] = useState", generated)
        self.assertIn("setCount(count + 1);", generated)
        self.assertIn('aria-label="name"', generated)
        self.assertIn('onChange={event => setName(event.target.value)}', generated)
        self.assertIn("{`Count: ${count}`}", generated)
        self.assertIn("{Boolean(count === 0) ? (", generated)
        self.assertIn('{"Ready to begin"}', generated)
        self.assertIn('{"Work in progress"}', generated)
        self.assertIn("export default Counter;", generated)

    def test_compiles_when_without_else(self):
        generated = compile_source(
            """app notice
profile react
component Notice:
  state visible: Bool = true
  view:
    column:
      heading "Notice"
      when visible:
        text "Visible"
""",
            target="react",
        )
        self.assertIn("{Boolean(visible) && (", generated)

    def test_rejects_orphan_else(self):
        with self.assertRaisesRegex(CompileError, "else must immediately follow a when block"):
            compile_source(
                """app bad
profile react
component Bad:
  view:
    column:
      text "before"
      else:
        text "after"
""",
                target="react",
            )

    def test_react_requires_react_profile(self):
        with self.assertRaisesRegex(CompileError, "requires 'profile react'"):
            compile_source(
                """app bad
profile script
component Bad:
  view:
    text "bad"
""",
                target="react",
            )

    def test_rejects_unknown_state_in_event(self):
        with self.assertRaisesRegex(CompileError, "unknown state 'missing'"):
            compile_source(
                """app bad
profile react
component Bad:
  view:
    button "Bad":
      on click:
        set missing = 1
""",
                target="react",
            )

    def test_controlled_input_requires_text_state(self):
        with self.assertRaisesRegex(CompileError, "input state must have type Text"):
            compile_source(
                """app bad
profile react
component Bad:
  state count: Int = 0
  view:
    input count
""",
                target="react",
            )


if __name__ == "__main__":
    unittest.main()

