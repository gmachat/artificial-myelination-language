# Static-language backends

AML 0.8 includes standard-library-only backends for Rust, C, Java, C#, and
Swift. They consume the same typed AML IR as Go and Ruby.

Rust implements the structured core tier: native structs, `Vec<T>`,
`Option<T>`, record literals, field access, and list indexing. It is compiled
and executed in the conformance suite without third-party crates.

Java and C# generate structured source using Java records/`List<T>`/
`Optional<T>` and C# records/`List<T>`/nullable values. Their source shapes are
tested, while execution certification awaits installed Java and .NET toolchains.

Swift generates `Equatable` structs, arrays, and optionals. Both scalar and
structured output pass `swiftc` parser validation; executable certification is
pending a matching Swift compiler and SDK installation.

## Current conformance tier

The initial `scalar-core` tier supports:

- `Bool`, `Float`, `Int`, and `Text`;
- typed task parameters and returns;
- task calls and local `let` values;
- arithmetic, comparisons, Boolean expressions, and text concatenation;
- `if`/`else`, `show`, `fail`, `require`, and `ensure`;
- deterministic `main` entrypoints; and
- local AML modules before IR generation.

C implements structured core with native structs, generated typed list
containers, heap-backed list storage, and generated optional `{has, value}`
structs. It is compiled and executed as C11 in the conformance suite. Recursive
value types are rejected explicitly. List allocations currently have
process-lifetime ownership; deterministic deep cleanup is the next C-specific
ownership milestone.

Python, JavaScript, Ruby, Go, Rust, and C execute structured core; Java, C#,
and Swift generate structurally validated structured source.

## Commands

```sh
aml build app.aml --target rust -o app.rs
aml build app.aml --target c -o app.c
aml build app.aml --target java -o AMLProgram.java
aml build app.aml --target csharp -o AMLProgram.cs
aml build app.aml --target swift -o app.swift
```

Generated source uses only each target's standard library. Platform compilers
and runtimes are still required. No package manager dependency is introduced.

## Verification status

- C and Rust are compiled and executed in the AML test suite when their
  compilers are available.
- Swift is parser-validated when `swiftc` is available. On the current test
  machine, the installed Swift compiler and macOS SDK builds do not match, so
  executable Swift certification is pending a consistent toolchain.
- Java and C# source shapes are tested, but this machine has neither a Java
  runtime nor a .NET SDK. Execution certification remains pending those tools.

This distinction is intentional: generated source is not labeled fully
conformant merely because it looks plausible.
