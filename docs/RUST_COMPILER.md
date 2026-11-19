# Native Rust compiler

AML is migrating from a Python implementation to a native Rust compiler. This
is a staged compatibility migration: a target moves only after its generated
projects and diagnostics pass their existing contract tests.

## Current native scope

The Rust binary currently implements these features without calling or
embedding Python:

- `aml check` for `react` and `web`
- `aml check`, `aml build`, and `aml run` for JavaScript
- native AML IR 1.1 JSON with a resolved type on every expression
- dependency-free Ruby generation and `aml run --target ruby`
- standard-library-only Go generation and `aml run --target go`
- standard-library-only Rust generation and `aml run --target rust`
- C11 generation and `aml run --target c`
- Java generation and `aml run --target java` when a JDK is installed
- C# generation and `aml run --target csharp` with the .NET 8 SDK
- Swift generation and `aml run --target swift` with a matching Swift SDK
- full-stack React/Node generation with JSON or PostgreSQL storage
- local `use` module loading with cycle and duplicate-declaration checks
- machine-readable `check --json`, `targets --json`, and `reference --json`
- dependency-free `aml init`, `aml add`, and deterministic `aml lock`
- core apps, profiles, records, typed tasks, locals, output, failures,
  conditions, returns, and expression statements
- runtime type boundaries and `require` / `ensure` contracts
- `aml build` for React JSX and complete Vite web projects
- component props and runtime boundary checks
- typed component state
- headings, text, interpolation, rows, and columns
- controlled text input
- click events with state updates and console output
- conditional `when` / `else` views
- target-neutral expression AST with precedence, calls, indexing, fields,
  lists, and record literals
- nested `List[T]` and `Optional[T]` type parsing and validation
- shared semantic inference for arithmetic, comparisons, calls, collections,
  records, component state, contracts, and conditional views
- `aml format`, `aml reference`, and `aml targets`

The Python 0.8 implementation remains only as a compatibility oracle while the
native release is stabilized. Every documented compiler target and developer
workflow now has a Rust implementation; the Rust CLI never invokes Python.

## Build and use

```sh
cargo test
cargo build --release --locked
./target/release/aml --version
./target/release/aml check examples/full_web.aml --target web
./target/release/aml build examples/full_web.aml --target web -o build/rust-web
```

Install it on the current machine:

```sh
cargo install --path . --locked
```

Create a distributable archive:

```sh
./scripts/build-rust-release.sh
```

## Why the migration is staged

AML already has more than 5,000 lines of compiler and backend behavior.
Replacing everything at once would make regressions hard to locate. The
existing test suite acts as the language contract while Rust replaces one
vertical slice at a time.

The planned order is:

1. React and complete web projects — implemented.
2. Shared expression AST, types, and semantic inference — implemented.
3. Core declarations/statements, JavaScript, and native `run` — implemented.
4. Stable typed JSON IR emitted from the Rust AST — implemented.
5. Ruby backend — implemented.
6. Go backend — implemented.
7. Rust backend — implemented.
8. C backend — implemented.
9. Java backend — implemented.
10. C# backend — implemented.
11. Swift backend — implemented.
12. Full-stack generation — implemented.
13. Modules and JSON diagnostics — implemented.
14. Package manifests and lockfiles — implemented.
15. External backends, LSP, and Python output — implemented.
16. Retire the Python compatibility distribution after release certification.

## Native artifact

Release mode enables link-time optimization, one code-generation unit, and
symbol stripping. The development binary remains small and links only to the
operating system library. Binary size and architecture vary by platform.

