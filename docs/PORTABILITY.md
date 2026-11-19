# AML portability and dependency policy

AML source is target-neutral. The native Rust compiler has no third-party
crate dependencies and emits AML IR without Python. The legacy Python compiler
is retained only as a temporary parity oracle.

## Universal backend boundary

```sh
aml build app.aml --target ir -o app.amlir.json
```

This produces versioned JSON AML IR. IR 1.1 is semantically typed: every
expression carries its resolved AML type after shared name, call, operator,
record, list, optional, condition, contract, and return validation. A backend
for C, C++, C#, Go, Java, Kotlin,
Rust, Swift, PHP, Ruby, or another language consumes this representation. It
does not need to parse AML or reproduce AML's precedence and literal rules.
The IR contains typed declarations, structured statements, and expression
trees rather than target-language text.

An external backend is any executable that implements `aml-backend-1`:

```sh
aml build app.aml --backend ./aml-backend-go -o build/go-app
```

AML starts the executable directly, without a shell. It sends a compact JSON
request containing `protocol`, `source`, and the complete `ir` to standard
input. The backend returns JSON on standard output:

```json
{
  "protocol": "aml-backend-1",
  "entry": "main.go",
  "artifacts": [
    {"path": "main.go", "content": "package main\n..."}
  ],
  "diagnostics": []
}
```

Artifact paths must be relative, unique, and remain inside the output
directory. AML rejects absolute paths and `..` traversal. An artifact may set
`"executable": true`, and a response may contain a complete multi-file project.

Use `aml targets` or `aml targets --json` to inspect which targets need runtime
platforms or application packages.

Before distributing a backend, run its protocol certification:

```sh
aml backend-check ./aml-backend-go --json
```

The check sends scalar, structured-data, and contract/control-flow fixtures
twice. It validates the response, rejects unsafe paths, enforces a process
timeout, and requires deterministic artifacts. This certifies the backend
boundary; target execution tests are still required for semantic conformance.

Backends must preserve AML contracts, exact task-boundary validation, failure
shape, and observable behavior. A target is not considered supported until it
passes the shared conformance suite.

## What “no dependencies” means

- The AML language does not inherit the syntax or package model of a target.
- The core Python package has zero mandatory third-party runtime dependencies.
- The JSON IR is usable with any implementation language.
- Generated core programs should prefer their target's standard library.

The built-in Go backend demonstrates this boundary with a static compiled
target. It consumes AML IR, embeds only a small generated validation runtime,
imports only Go standard-library packages, and can produce a standalone native
executable with `go build`.

Rust, C, Java, C#, and Swift are also built-in IR consumers. Their initial
scalar-core tier deliberately rejects structured types until target-native
records, lists, and optionals pass conformance. See `STATIC_BACKENDS.md`.

It cannot mean that programs run without a platform. Python output needs
Python, JavaScript output needs a JavaScript runtime, and native output needs a
compiler or a prebuilt executable. React and full-stack profiles intentionally
generate ecosystem applications and therefore declare their application
dependencies.

The Rust frontend now owns all built-in targets, React, web, full-stack,
packages, external backends, typed IR, and the language server.

## Backend acceptance rules

1. Consume the stable AML IR rather than inventing target-specific AML syntax.
2. Require no third-party runtime library for the core `script` profile.
3. Pass the same records, lists, optionals, contracts, and failure tests.
4. Produce deterministic output from identical IR.
5. Document unavoidable platform tools separately from application packages.
6. Measure authored AML tokens and complete AI workflow tokens before claiming
   an efficiency improvement.
