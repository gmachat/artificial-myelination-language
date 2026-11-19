import assert from "node:assert/strict";
import { mkdtemp, readFile, writeFile } from "node:fs/promises";
import os from "node:os";
import path from "node:path";
import test from "node:test";
import { fileURLToPath } from "node:url";
import { evaluateFiles, listTasks, prepare, report, runBenchmark, runSuite } from "./benchmark.mjs";

test("catalog contains ten distinct full-stack tasks", async () => {
  const tasks = await listTasks();
  assert.equal(tasks.length, 10);
  assert.equal(new Set(tasks.map((task) => task.id)).size, 10);
  assert.ok(tasks.some((task) => task.fields.some((field) => field.type === "Time")));
  assert.ok(tasks.some((task) => task.fields.some((field) => field.type === "Float")));
});

test("prepare includes AML reference only in AML condition", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "aml-ai-prepare-"));
  await prepare(root);
  const aml = await readFile(path.join(root, "aml/prompt.md"), "utf8");
  const typescript = await readFile(path.join(root, "typescript/prompt.md"), "utf8");
  assert.match(aml, /AML AI REFERENCE/);
  assert.doesNotMatch(typescript, /AML AI REFERENCE/);
});

test("AML structural evaluation compiles a generic fullstack program", async () => {
  const result = await evaluateFiles("aml", { "main.aml": "app inventory\nprofile fullstack\n\ndata Product:\n  name: Text\n  quantity: Int\n  active: Bool\n\nresource products: Product:\n  list\n  create\n  update\n  delete\n\npage Dashboard at \"/\":\n  resource products\n" });
  assert.equal(result.passed, true);
});

test("task-specific evaluation checks a non-inventory schema", async () => {
  const task = (await listTasks()).find((candidate) => candidate.id === "expenses");
  const source = "app expenses\nprofile fullstack\n\ndata Expense:\n  description: Text\n  amount: Float\n  reimbursed: Bool\n\nresource expenses: Expense:\n  list\n  create\n  update\n  delete\n\npage Expenses at \"/\":\n  resource expenses\n";
  const result = await evaluateFiles("aml", { "main.aml": source }, { task });
  assert.equal(result.passed, true);
});

test("report counts all repair rounds and requires equal models", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "aml-ai-report-"));
  await prepare(root);
  const evaluation = { level: "structural", passed: true, checks: [] };
  await writeFile(path.join(root, "aml/usage.json"), JSON.stringify({ model: "same", rounds: [{ input_tokens: 10, output_tokens: 5, reasoning_tokens: 1, total_tokens: 15 }, { input_tokens: 6, output_tokens: 2, reasoning_tokens: 0, total_tokens: 8 }] }));
  await writeFile(path.join(root, "typescript/usage.json"), JSON.stringify({ model: "same", rounds: [{ input_tokens: 30, output_tokens: 10, reasoning_tokens: 0, total_tokens: 40 }] }));
  await writeFile(path.join(root, "aml/evaluation.json"), JSON.stringify(evaluation));
  await writeFile(path.join(root, "typescript/evaluation.json"), JSON.stringify(evaluation));
  const result = await report(root);
  assert.equal(result.conditions.aml.repair_rounds, 1);
  assert.equal(result.conditions.aml.totals.total_tokens, 23);
  assert.equal(result.comparison.savings_percent, 42.5);
});

test("adapter protocol runs evaluation, feedback, and repair", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "aml-ai-adapter-"));
  const adapter = path.join(path.dirname(fileURLToPath(import.meta.url)), "mock_adapter.mjs");
  const result = await runBenchmark(root, adapter, 2);
  assert.equal(result.conditions.aml.evaluation.passed, true);
  assert.equal(result.conditions.aml.repair_rounds, 1);
  assert.equal(result.conditions.typescript.evaluation.passed, true);
  assert.equal(result.conditions.typescript.repair_rounds, 0);
  assert.equal(result.conditions.aml.model, "mock/not-a-real-model");
  assert.equal(result.comparison.savings_percent, null);
});

test("suite runs selected tasks and refuses synthetic savings", async () => {
  const root = await mkdtemp(path.join(os.tmpdir(), "aml-ai-suite-"));
  const adapter = path.join(path.dirname(fileURLToPath(import.meta.url)), "mock_adapter.mjs");
  const result = await runSuite(root, adapter, 2, 1, false, "docker", ["inventory", "expenses"]);
  assert.equal(result.scheduled_runs, 2);
  assert.deepEqual(result.passed, { aml: 2, typescript: 2 });
  assert.equal(result.comparison.savings_percent, null);
  assert.equal(JSON.parse(await readFile(path.join(root, "suite-report.json"), "utf8")).schema, "aml-ai-benchmark-suite-report-1");
});
