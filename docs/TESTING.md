# AML testing and repair

AML tests are ordinary executable AML programs stored beneath a test directory.
A test passes when its `main` task completes successfully. Use `require` and
`ensure` contracts for assertions; a failed contract makes the test fail.

```aml
app math_test
profile script

task double(value: Int) -> Int:
  return value * 2

task verify_double:
  require double(21) == 42

task main:
  verify_double()
```

Run every `.aml` file recursively beneath `tests`:

```sh
aml test
aml test tests --target javascript --json
```

The native runner returns schema `aml-test-report-1`, with deterministic case
ordering, captured output, captured errors, and pass/fail totals. Each test is a
separate process so one failed contract does not stop later tests.

## Repair packets

For a compact diagnostic intended for an AI coding agent:

```sh
aml diagnose main.aml --target javascript --json
aml agent repair-context main.aml --target javascript
```

Both commands produce schema `aml-repair-context-1`. The packet includes the
error code, category, exact location, a small numbered source window, inferred
expected/actual values when available, a correction category, token estimate,
and canonical verification command. It does not include unrelated files.

The MCP server exposes the same operations as `aml_diagnose`,
`aml_repair_context`, and `aml_test`.

