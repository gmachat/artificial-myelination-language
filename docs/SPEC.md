# AML core specification (draft 0.8)

This document describes the executable prototype, not the complete future
language.

## Source format

- Source files end in `.aml`.
- Files are UTF-8.
- Indentation is exactly two spaces per block level.
- Tabs are invalid.
- Blank lines and lines beginning with `#` are ignored.
- Identifiers use `snake_case`; type names use `PascalCase` by convention.

## Application

Every file begins with exactly one application declaration:

```aml
app example
```

The declaration gives tooling a stable identity and is not a runtime statement.

## Profile

Every application selects exactly one profile before its first task:

```aml
profile script
```

Profiles supply canonical behavior and declare expected capabilities:

| Profile | Capabilities |
| --- | --- |
| `script` | console |
| `local` | console, files |
| `connected` | console, files, network |
| `react` | browser, UI |
| `web` | browser, UI, network |
| `fullstack` | browser, UI, network, server, storage |

All 0.8 profiles provide runtime parameter type validation, executable
contracts, compact structured failures, and contract metadata. Future releases
will connect file and network capabilities to standard operations and enforce
them at compile time.

## Tasks

A task is AML's only callable declaration:

```aml
task add(left: Int, right: Int) -> Int:
  return left + right
```

Parameters require types. The return type is optional. A task without a return
type returns `None`. A file may define one parameterless `main` task, which the
generated program calls automatically.

Scalar types are `Any`, `Bool`, `Float`, `Int`, and `Text`. Core tasks also
support `List[T]`, `Optional[T]`, and named records:

```aml
record Person:
  name: Text
  scores: List[Int]
  nickname: Optional[Text]
```

Records appear before tasks. Values use `{name: "Ada", scores: [42],
nickname: none}` and fields use `person.name`. Python and JavaScript validate
the complete nested value at task boundaries, including exact record fields.

## Contracts

Tasks may state preconditions and postconditions without handwritten checking:

```aml
task percentage(part: Int, whole: Int) -> Float:
  require whole > 0
  ensure result >= 0
  return part / whole * 100
```

- `require` checks a condition when execution reaches it.
- `ensure` declares a task-level postcondition and must mention `result`.
- Every returned value is checked against every `ensure` declaration.
- Contract failures use one stable, machine-readable diagnostic shape.
- The compiler exposes declarations in `__aml_contracts__` for agent tools.

## Statements

```aml
let total = price * quantity
show total
return total
fail "calculation unavailable"
if total > 100:
  show "large"
else:
  show "standard"
```

- `let` creates a local name. Reassignment is intentionally absent in 0.1.
- `show` writes one value followed by a newline.
- `return` exits the current task.
- `if` and `else` provide conditional execution.
- A bare expression is allowed for calling a task whose result is unused.

Expressions use one shared target-neutral parser: literals, names, lists,
records, indexing, field access, arithmetic, comparisons, `and`, `or`, `not`,
and task calls. The AML literals `true`, `false`, and `none` compile to their
target equivalents.

## Compilation targets

AML 0.8 provides portable IR plus built-in source and project backends:

- `python` is the default and produces Python 3 source.
- `javascript` (or its `js` alias) produces dependency-free JavaScript for
  modern Node.js.
- `ruby` consumes AML IR and produces Ruby with no third-party gem dependency.
- `go` consumes AML IR and produces standard-library-only Go source.
- `rust` consumes typed AML IR and implements standard-library-only structured
  core using structs, vectors, and options.
- `java` and `csharp` generate structured source using their native record,
  collection, and optional/nullable facilities.
- `swift` generates structured source using structs, arrays, and optionals.
- `c` implements structured core using native structs and generated typed list
  and optional containers; recursive value types are not yet supported.
- `react` produces a typed JSX component for an existing React project.
- `web` produces a complete Vite + React application directory.
- `fullstack` produces a React dashboard, REST API, persistent store, and Node
  production server.
- `ir` produces versioned, target-neutral JSON for implementing additional
  language backends without another AML parser. IR 1.1 includes resolved types
  on every expression and is emitted only after semantic validation.

The Python, JavaScript, Ruby, and Go backends implement task calls, structured
types, boundary validation, contracts, structured failures, conditions,
output, and the `main` entrypoint.
AML's `Int` maps to a JavaScript integer-valued `number`; `Float` maps to any
finite JavaScript `number`.

## Full-stack data

Full-stack applications use JSON persistence by default or select PostgreSQL:

```aml
profile fullstack
storage postgres

data Game:
  league: Text
  starts_at: Time
  edge: Float
  active: Bool

resource games: Game:
  list
  create
  update
  delete
```

Persistent types are `Text`, `Int`, `Float`, `Bool`, and `Time`. PostgreSQL
projects include a tracked SQL migration, connection pooling, parameterized
queries, timestamps, and a migration command. JSON projects retain their
zero-configuration local store.

## Canonical CLI

```text
aml check <file>             validate source
aml check <file> --json      emit machine-readable diagnostics
aml format <file> [--check]  apply or verify canonical formatting
aml reference [--json]       print the compact AI reference
aml lsp                      run the editor language server
aml init [directory]         create an AML package
aml add <name> <path>        add and lock a local dependency
aml lock                     regenerate aml.lock.json
aml build <file> [-o path]   compile source to Python
aml run <file>               compile and execute source
aml build <file> --target javascript [-o path]
aml run <file> --target javascript
aml check <file> --target react
aml build <file> --target react [-o Component.jsx]
aml check <file> --target web
aml build <file> --target web [-o project-directory]
aml check <file> --target fullstack
aml build <file> --target fullstack [-o project-directory]
```

Diagnostics use `path:line:column: message`, which is concise and easy for an
agent to locate and repair.

