# End-to-end AI workflow benchmark

This benchmark compares one model completing the same applications in AML and
TypeScript. Its first public cohort contains ten full-stack projects with varied
Text, Int, Float, Bool, and Time schemas. It counts provider-reported usage
across the initial generation and every repair round.

List the versioned task catalog:

```sh
node benchmarks/ai_workflow/benchmark.mjs list
```

## Run with a model adapter

Implement the small executable protocol in `adapter_protocol.md`, then run:

```sh
node benchmarks/ai_workflow/benchmark.mjs run \
  --adapter /absolute/path/to/model-adapter \
  --run-dir benchmarks/ai_workflow/runs/model-trial-1 \
  --task expenses \
  --max-repairs 2
```

The same adapter executable—and therefore the same provider, model version,
and model settings—serves both conditions. AML's complete compact reference is
included in AML input usage. All previous files and evaluator feedback are
included on repairs, so their tokens are not discarded.

## Run the complete suite

Run all ten tasks once:

```sh
node benchmarks/ai_workflow/benchmark.mjs suite \
  --adapter /absolute/path/to/model-adapter \
  --run-dir benchmarks/ai_workflow/runs/suite-1 \
  --repetitions 1 \
  --max-repairs 2
```

Use `--tasks inventory,expenses,events` for a subset. A stronger trial should
use at least three repetitions, but that invokes the model twice per task per
repetition plus any repairs. With ten tasks and three repetitions, the minimum
is 60 billable model generations. The suite report includes total and average
tokens, generation and evaluation time, pass rate, first-pass rate, repair
rounds, per-task results, and a weighted savings percentage.

Suite savings are withheld unless every scheduled AML/TypeScript pair passes at
the same evaluation level, all calls use one non-mock model, and provider usage
is available. This prevents cherry-picking only successful tasks.

Two optional zero-package adapters are included. They require explicit model
names so trials cannot silently change models:

### OpenAI local setup

Create a project API key in the OpenAI dashboard. Put it only in the ignored
local file `benchmarks/ai_workflow/.env.openai`, replacing this value:

```text
OPENAI_API_KEY=paste_your_openai_api_key_here
```

The file and a safe loader are already present. Do not paste the key into AML
source, a prompt, this README, or a benchmark run directory. Confirm the setup
without making an API call:

```sh
node benchmarks/ai_workflow/openai_runner.mjs check
```

Then run the inexpensive three-task smoke trial:

```sh
node benchmarks/ai_workflow/openai_runner.mjs suite \
  --run-dir benchmarks/ai_workflow/runs/openai-smoke-1 \
  --tasks inventory,expenses,events \
  --repetitions 1 \
  --max-repairs 2
```

The runner pins the model from `OPENAI_MODEL` in the same local file and selects
`openai_adapter.mjs` automatically. The suite command makes billable API calls.

You may alternatively export the variables directly:

```sh
export OPENAI_API_KEY="..."
export OPENAI_MODEL="YOUR_EXACT_MODEL"
node benchmarks/ai_workflow/benchmark.mjs run \
  --adapter benchmarks/ai_workflow/openai_adapter.mjs \
  --run-dir benchmarks/ai_workflow/runs/openai-trial-1
```

```sh
export ANTHROPIC_API_KEY="..."
export ANTHROPIC_MODEL="YOUR_EXACT_MODEL"
node benchmarks/ai_workflow/benchmark.mjs run \
  --adapter benchmarks/ai_workflow/anthropic_adapter.mjs \
  --run-dir benchmarks/ai_workflow/runs/anthropic-trial-1
```

These commands make billable API calls. Keep credentials in environment
variables; never put them in run directories or committed files.

## Isolated runtime evaluation

Structural evaluation is the safe default. To require both applications to
build and pass the same black-box API, persistence, validation, deletion, and
browser-serving checks, install Docker or Podman and opt in explicitly:

```sh
node benchmarks/ai_workflow/benchmark.mjs run \
  --adapter /absolute/path/to/model-adapter \
  --run-dir benchmarks/ai_workflow/runs/runtime-trial-1 \
  --execute \
  --engine docker
```

Use `--engine podman` for Podman. The harness verifies the engine before it
invokes the adapter, so a missing engine cannot consume model tokens. It
replaces package scripts, rejects dependencies outside a small allowlist,
installs packages with lifecycle scripts disabled, then runs generated code in
a container with no network, a read-only container filesystem, all Linux
capabilities dropped, `no-new-privileges`, and CPU, memory, and process limits.
The temporary project bind mount remains writable so the application can prove
file persistence across a server restart.

Package installation is a separate phase with network access. Do not treat this
as a perfect hostile-code boundary: keep the container engine patched and run
the benchmark on a disposable machine for adversarial submissions.

## Manual workflow

Prepare exact prompts:

```sh
node benchmarks/ai_workflow/benchmark.mjs prepare --run-dir RUN
```

Select a task with `--task TASK_ID`; inventory is the compatibility default.

Put `response.json`, `usage.json`, and later `evaluation.json` beneath each
condition directory using the schemas produced by an automated run. Then use:

```sh
node benchmarks/ai_workflow/benchmark.mjs evaluate --run-dir RUN
node benchmarks/ai_workflow/benchmark.mjs report --run-dir RUN
```

Add `--execute --engine docker` to `evaluate` to replace an existing structural
evaluation with runtime evaluation.

## Interpretation

The report emits savings only when both conditions use the identical model and
pass the same evaluation level. The default structural level compiles AML and
checks the authored TypeScript project. The opt-in runtime level additionally
requires both generated applications to pass identical behavioral tests. It is
stronger evidence than source size or structural checks. The current ten-task
cohort covers one architecture—React plus a persisted HTTP API—so it is not
proof that either language is better for every category of software.

`mock_adapter.mjs` exists only to test the harness and repair loop. Its synthetic
token values must never be quoted as AML performance results.

