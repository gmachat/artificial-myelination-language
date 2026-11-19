# AML — Artificial Myelination Language

AML is an experimental, agent-native programming language. Its goal is to let
AI coding agents express intent with fewer decisions, less boilerplate, and
more predictable project navigation—while remaining readable to people.

This repository contains the runnable prototype. AML compiles to Python,
JavaScript, Ruby, Go, Rust, C, Java, C#, Swift, React JSX, complete Vite web
projects, and full-stack React + Node applications with persistent storage.

## First program

```aml
app hello
profile script

task greet(name: Text) -> Text:
  return "Hello, " + name + "!"

task main:
  let message = greet("AML")
  show message
```

## Install

### Native Rust compiler

AML's primary compiler is a standalone native Rust binary. Download the archive
for your platform, extract it, and run its bundled installer:

```sh
./install.sh
aml --version
aml doctor
```

On Windows, use `.\install.ps1` in PowerShell. See
[docs/INSTALLING.md](docs/INSTALLING.md) for custom locations and uninstall.
To install from a source checkout instead:

```sh
cargo install --path .
aml check examples/full_web.aml --target web
aml build examples/full_web.aml --target web -o build/aml-counter
aml run examples/hello.aml
aml run examples/hello.aml --target ruby
aml run examples/hello.aml --target go
```

Build a small platform archive for other users with:

```sh
./scripts/build-rust-release.sh
```

The native compiler contains the shared expression parser, nested type parser,
semantic inference, every built-in backend, package tooling, external backend
protocol, and language server. It never invokes Python. See
[docs/RUST_COMPILER.md](docs/RUST_COMPILER.md) for certification details.

### Legacy Python compatibility compiler

The previous implementation remains temporarily available for parity testing.
New installations should use the native archive. From a checked-out copy:

```sh
python3 -m pip install .
```

For an isolated command-line installation, use `pipx`:

```sh
pipx install .
```

Once AML is published to a package registry, the intended command will be:

```sh
pipx install artificial-myelination-language
```

Confirm the installation:

```sh
aml --version
```

Run the example:

```sh
aml run examples/hello.aml
```

Create a new project with a canonical structure:

```sh
aml new hello
cd hello
aml check main.aml --json
aml run main.aml
```

Generate a compact, provider-neutral context packet for ChatGPT, Claude,
Copilot, or a local coding model:

```sh
aml agent context --json > aml-context.json
```

See [docs/AI_INTEGRATION.md](docs/AI_INTEGRATION.md) for the safety limits and
recommended agent workflow.

Connect AML directly to an MCP-compatible AI host:

```sh
aml mcp --project /absolute/path/to/project --print-config
```

The native server exposes context, check, format, build, doctor, and target
tools. See [docs/MCP.md](docs/MCP.md) for configuration and its project-root
security boundary.

Run AML tests and request a compact repair packet:

```sh
aml test tests --target javascript --json
aml diagnose main.aml --target javascript --json
aml agent repair-context main.aml --target javascript
```

See [docs/TESTING.md](docs/TESTING.md) for contract-based tests and structured
repair reports.

Other commands:

```sh
aml check examples/hello.aml
aml check examples/hello.aml --json
aml test examples/testing/tests --json
aml diagnose examples/hello.aml --json
aml format examples/hello.aml --check
aml reference
aml doctor
aml mcp --project . --print-config
aml build examples/hello.aml -o build/hello.py
aml build examples/hello.aml --target javascript -o build/hello.js
aml build examples/hello.aml --target ruby -o build/hello.rb
aml build examples/hello.aml --target go -o build/hello.go
aml build examples/hello.aml --target rust -o build/hello.rs
aml build examples/hello.aml --target c -o build/hello.c
aml build examples/hello.aml --target java -o build/AMLProgram.java
aml build examples/hello.aml --target csharp -o build/AMLProgram.cs
aml build examples/hello.aml --target swift -o build/hello.swift
aml build examples/hello.aml --target ir -o build/hello.amlir.json
aml build examples/hello.aml --backend ./aml-backend-go -o build/hello-go
aml targets --json
aml backend-check ./aml-backend-go --json
aml run examples/hello.aml --target javascript
aml run examples/hello.aml --target rust
aml run examples/hello.aml --target c
aml run examples/hello.aml --target java
aml run examples/hello.aml --target csharp
aml run examples/hello.aml --target swift
aml build examples/counter.aml --target react -o build/Counter.jsx
aml build examples/full_web.aml --target web -o build/aml-counter
aml build examples/postgres_fullstack.aml --target fullstack -o build/dashboard
python3 -m unittest discover -s tests
```

Multi-file applications use a top-level local import:

```aml
use "math.aml"
```

The native compiler resolves imports recursively and rejects module cycles.

Create and connect reusable packages without Python:

```sh
aml init utility --name utility
aml add utility ../utility --project ./application
aml lock --project ./application
```

JavaScript output targets modern Node.js and has no runtime package dependency.
The generated file can be run directly with `node build/hello.js`.
Ruby output is generated from AML IR and uses no third-party gems.
Go output is generated from the same IR, uses only the Go standard library,
and can be compiled into a standalone native executable with `go build`.
Rust and C provide standard-library-only structured core with native generated
types. Java, C#, and Swift generate structured source using native records or
structs and collection/optional facilities. See
[docs/STATIC_BACKENDS.md](docs/STATIC_BACKENDS.md) for execution-certification
and C ownership details.
React components use `profile react` and are built into an existing React
project. See [docs/REACT.md](docs/REACT.md) for the component syntax.
Use `profile web` with `--target web` to generate an entire runnable Vite app.
See [docs/WEB.md](docs/WEB.md) for installation, development, and production
build commands.
Use `profile fullstack` to generate a React dashboard, REST API, validation,
storage, and combined production server. See
[docs/FULLSTACK.md](docs/FULLSTACK.md).

The native compiler generates it directly:

```sh
aml build examples/postgres_fullstack.aml --target fullstack -o build/dashboard
```

Maintainers can build native platform archives using the instructions in
[docs/RELEASING.md](docs/RELEASING.md).

Maintainers can run cross-target executable checks with
`node scripts/conformance.mjs`. A VS Code extension backed by `aml lsp` lives
in `editors/vscode`; its README explains local `.vsix` packaging.

AML can also be shipped as one native Rust executable so end users do not need
Python. See [docs/RUST_COMPILER.md](docs/RUST_COMPILER.md) for the native
compiler and [docs/STANDALONE.md](docs/STANDALONE.md) for the earlier packaged
Python release process.

After extracting a standalone release:

```sh
./aml --version
./aml run app.aml
```

The install has no mandatory third-party Python dependencies. AML's versioned
JSON IR is the universal backend boundary for adding languages without changing
AML syntax or writing another AML parser. See
[docs/PORTABILITY.md](docs/PORTABILITY.md) for the dependency policy and the
honest distinction between a zero-dependency compiler core and platform or
framework requirements of generated programs.

## Measure source-token reduction

AML includes paired AML, Python, JavaScript, Ruby, and Go reference programs:

```sh
aml benchmark
aml benchmark --language javascript
aml benchmark --language ruby
aml benchmark --language go
aml benchmark --json
```

The benchmark reports authored-source size and labels whether it used an exact
tokenizer or an approximation. It does not claim to measure AI reasoning or
repair tokens. See [docs/BENCHMARKING.md](docs/BENCHMARKING.md) for the complete
evaluation methodology.

For complete model-reported input, output, reasoning, and repair-round usage:

```sh
node benchmarks/ai_workflow/benchmark.mjs run \
  --adapter /absolute/path/to/model-adapter \
  --run-dir benchmarks/ai_workflow/runs/trial-1
```

The adapter is provider-neutral; see
[benchmarks/ai_workflow/README.md](benchmarks/ai_workflow/README.md).
Add `--execute --engine docker` (or `podman`) to require isolated, identical
runtime behavior tests before the harness reports a runtime-level comparison.
Use the `suite` command with `--repetitions 3` to run the ten-project cohort and
produce aggregate pass-rate, repair, and token-savings statistics.

## Design rules

1. **One obvious form.** AML avoids aliases and competing styles.
2. **Intent over plumbing.** Common validation, cleanup, and error behavior
   will be supplied by profiles and the compiler.
3. **Visible effects.** I/O operations use recognizable verbs such as `show`.
4. **Stable structure.** Agents should know where declarations and entrypoints
   live without searching a large repository.
5. **Small context footprint.** The complete core language should fit in an
   AI agent's prompt without extensive documentation retrieval.
6. **Deterministic tooling.** Formatting, checking, building, and testing have
   canonical commands and output.

See [docs/VISION.md](docs/VISION.md) for the product direction and
[docs/SPEC.md](docs/SPEC.md) for the current language specification.

## Status

AML is at version `0.8.0.dev0`. The prototype supports applications, local
modules, deterministic package locks, typed tasks, records, lists, optional
values, variables, conditions, executable contracts, profiles, Python and
JavaScript compilation, React/web/full-stack generation, JSON diagnostics,
canonical formatting, an editor language server, and semantically typed IR
1.1 shared by language backends. It remains intentionally
small so an AI can learn the usable language from `aml reference` in under 500
estimated tokens.

Structured types are concise and checked recursively in Python and JavaScript:

```aml
record Person:
  name: Text
  scores: List[Int]
  nickname: Optional[Text]

task best(person: Person) -> Int:
  return person.scores[0]
```

## Profiles and contracts

One profile replaces a bundle of repeated setup decisions:

```aml
profile script
```

The built-in profiles are `script`, `local`, and `connected`. They declare the
capabilities an application category normally needs. All profiles currently
enable runtime type validation, structured failures, and contract checks.

Tasks express their boundaries directly:

```aml
task percentage(part: Int, whole: Int) -> Float:
  require whole > 0
  ensure result >= 0
  return part / whole * 100
```

The compiler generates the validation and exposes contracts as structured
metadata for future agent tooling.

## End-to-end AI token experiment

The source-size benchmark is complemented by a matched first-pass AI experiment
that compares AML with direct JavaScript generation using the same full-stack
sports task and correctness tests:

```sh
cd experiments/ai_token_benchmark
python3 benchmark.py prepare --run-dir runs/manual-1
```

The harness can call the OpenAI Responses API, record reported input, output,
reasoning, and total tokens, compile AML, and test both generated applications.
See [experiments/ai_token_benchmark/README.md](experiments/ai_token_benchmark/README.md)
for the safety boundary and commands.

