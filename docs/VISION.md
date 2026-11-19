# AML vision

## Name

**AML** means **Artificial Myelination Language**.

Myelination helps biological signals travel efficiently. AML applies that
metaphor to software creation: repeated engineering decisions become reliable
language pathways, allowing an AI agent to move from intent to verified code
with less deliberation.

## Problem

Today's general-purpose languages make an agent repeatedly decide how to lay
out a project, choose libraries, validate input, represent failures, configure
tools, and connect components. Source code may be concise while the reasoning
needed to produce and verify it remains expensive.

AML aims to minimize the total cost of producing correct software:

```text
total cost = prompt tokens + source tokens + repair tokens + verification work
```

Short syntax alone is not enough. AML should reduce ambiguity and repair loops.

## Intended user

AML is written primarily by AI coding agents and reviewed, guided, and
maintained by people. Human readability remains a requirement because compact
but opaque machine syntax makes review and debugging expensive.

## Long-term shape

AML should eventually provide:

- canonical project and module layouts;
- contracts describing inputs, outputs, effects, and failure modes;
- safe profiles for common application categories;
- batteries-included HTTP, data, file, test, and model operations;
- compiler-generated validation, retries, tracing, and cleanup;
- deterministic dependency selection and reproducible builds;
- structured diagnostics designed for automatic repair;
- a compact machine-readable language reference;
- multiple targets, beginning with Python and later native code or WebAssembly.

## What AML is not

AML is not code golf, a prompt format, or an English-like language that guesses
what the author meant. It should be compact because defaults are explicit and
stable, not because behavior is mysterious.

## Success measures

AML should be benchmarked against mainstream languages on realistic tasks:

- tokens required to reach a passing implementation;
- first-attempt task success;
- number of compiler and test repair loops;
- defects and security findings;
- human review time;
- runtime and build performance.

An AML feature is valuable when it improves total task performance, not merely
when it removes characters.

