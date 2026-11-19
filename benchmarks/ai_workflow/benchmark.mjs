#!/usr/bin/env node
import { spawn } from "node:child_process";
import { mkdtemp, mkdir, readFile, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";
import { containerEngineAvailable, prepareRuntimeWorkspace, runtimeEvaluate } from "./runtime_evaluator.mjs";

export const PROTOCOL = "aml-ai-adapter-1";
const here = path.dirname(fileURLToPath(import.meta.url));
const repository = path.resolve(here, "../..");
const conditions = ["aml", "typescript"];
const MAX_RESPONSE_BYTES = 4 * 1024 * 1024;
let taskCatalog;

export async function listTasks() {
  if (!taskCatalog) {
    const catalog = JSON.parse(await readFile(path.join(here, "tasks.json"), "utf8"));
    if (catalog.schema !== "aml-ai-task-catalog-1" || !Array.isArray(catalog.tasks) || !catalog.tasks.length) throw new Error("invalid task catalog");
    const ids = new Set();
    for (const task of catalog.tasks) {
      if (!/^[a-z][a-z0-9_-]*$/.test(task.id) || ids.has(task.id)) throw new Error(`invalid or duplicate task id: ${task.id}`);
      ids.add(task.id);
      if (!/^[A-Z][A-Za-z0-9_]*$/.test(task.model) || !/^[a-z][a-z0-9_]*$/.test(task.resource)) throw new Error(`invalid model or resource in task: ${task.id}`);
      if (!Array.isArray(task.fields) || !task.fields.length) throw new Error(`task has no fields: ${task.id}`);
      for (const field of task.fields) if (!/^[a-z][a-z0-9_]*$/.test(field.name) || !["Text", "Int", "Float", "Bool", "Time"].includes(field.type)) throw new Error(`invalid field in task: ${task.id}`);
    }
    taskCatalog = catalog.tasks;
  }
  return taskCatalog;
}

async function getTask(id = "inventory") {
  const task = (await listTasks()).find((candidate) => candidate.id === id);
  if (!task) throw new Error(`unknown task '${id}'; use 'list' to see task ids`);
  return task;
}

function taskPrompt(task) {
  const fields = task.fields.map((field) => `- \`${field.name}\`: ${field.type}${field.type === "Text" ? ", non-empty" : ""}`).join("\n");
  return `# ${task.title} benchmark\n\nBuild a complete ${task.title.toLowerCase()} web application.\n\nRequired record \`${task.model}\` fields:\n\n${fields}\n\nRequired behavior:\n\n- A browser page lists ${task.resource}.\n- A form creates and updates a ${task.model.toLowerCase()}.\n- A control deletes a ${task.model.toLowerCase()}.\n- The API uses GET and POST at \`/api/${task.resource}\` and PUT and DELETE at \`/api/${task.resource}/:id\`.\n- Invalid types, missing fields, empty text, and unknown fields are rejected.\n- Records receive unique identifiers and persist across server restarts.\n- The project provides development, production-build, and production-start commands.\n- The production server hosts both the API and compiled browser application.\n\nReturn only one JSON object with a \`files\` object. Each key is a relative file path and each value is the complete UTF-8 file content. Do not include lockfiles, dependency directories, generated assets, markdown fences, or commentary.`;
}

function safeFiles(files) {
  if (!files || typeof files !== "object" || Array.isArray(files)) throw new Error("response files must be an object");
  const entries = Object.entries(files);
  if (!entries.length || entries.length > 100) throw new Error("response must contain 1-100 files");
  let bytes = 0;
  for (const [name, content] of entries) {
    if (typeof content !== "string") throw new Error(`file ${name} content must be text`);
    const normalized = name.replaceAll("\\", "/");
    if (!name || path.isAbsolute(name) || normalized.split("/").includes("..") || normalized.startsWith(".")) throw new Error(`unsafe response path: ${name}`);
    bytes += Buffer.byteLength(content);
  }
  if (bytes > MAX_RESPONSE_BYTES) throw new Error("response files exceed 4 MiB");
  return Object.fromEntries(entries.sort(([left], [right]) => left.localeCompare(right)));
}

function usage(value) {
  if (!value || typeof value !== "object") throw new Error("adapter response needs usage");
  const output = {};
  for (const name of ["input_tokens", "output_tokens", "reasoning_tokens"]) {
    const count = value[name] ?? 0;
    if (!Number.isSafeInteger(count) || count < 0) throw new Error(`usage.${name} must be a nonnegative integer`);
    output[name] = count;
  }
  if (output.reasoning_tokens > output.output_tokens) throw new Error("usage.reasoning_tokens must be a subset of output_tokens");
  output.total_tokens = output.input_tokens + output.output_tokens;
  return output;
}

export async function prepare(runDirectory, taskId = "inventory") {
  const task = await getTask(taskId);
  const instructions = taskPrompt(task);
  const reference = await readFile(path.join(repository, "aml/reference.txt"), "utf8");
  await mkdir(runDirectory, { recursive: true });
  const prompts = {
    aml: `${instructions}\n\nUse AML only. Return exactly one file named main.aml.\n\n${reference}`,
    typescript: `${instructions}\n\nUse TypeScript, React, Vite, and a Node server. Return all authored project files.`,
  };
  for (const condition of conditions) {
    const directory = path.join(runDirectory, condition);
    await mkdir(directory, { recursive: true });
    await writeFile(path.join(directory, "prompt.md"), prompts[condition]);
  }
  await writeFile(path.join(runDirectory, "metadata.json"), `${JSON.stringify({ schema: "aml-ai-benchmark-run-2", task: task.id, task_definition: task, conditions }, null, 2)}\n`);
  return prompts;
}

async function invokeAdapter(executable, request, timeoutMs = 180_000) {
  return await new Promise((resolve, reject) => {
    const child = spawn(executable, [], { stdio: ["pipe", "pipe", "pipe"] });
    const stdout = [];
    const stderr = [];
    let size = 0;
    const timer = setTimeout(() => { child.kill(); reject(new Error(`adapter timed out after ${timeoutMs / 1000} seconds`)); }, timeoutMs);
    child.stdout.on("data", (chunk) => {
      size += chunk.length;
      if (size > MAX_RESPONSE_BYTES) { child.kill(); reject(new Error("adapter response exceeds 4 MiB")); }
      else stdout.push(chunk);
    });
    child.stderr.on("data", (chunk) => stderr.push(chunk));
    child.on("error", reject);
    child.on("close", (code) => {
      clearTimeout(timer);
      if (code !== 0) return reject(new Error(`adapter exited ${code}: ${Buffer.concat(stderr).toString("utf8").trim()}`));
      try { resolve(JSON.parse(Buffer.concat(stdout).toString("utf8"))); }
      catch (error) { reject(new Error(`adapter returned invalid JSON: ${error.message}`)); }
    });
    child.stdin.end(`${JSON.stringify(request)}\n`);
  });
}

async function runCommand(command, args, cwd, timeoutMs = 120_000) {
  return await new Promise((resolve) => {
    const child = spawn(command, args, { cwd, stdio: ["ignore", "pipe", "pipe"] });
    const stdout = [];
    const stderr = [];
    const timer = setTimeout(() => child.kill(), timeoutMs);
    child.stdout.on("data", (chunk) => stdout.push(chunk));
    child.stderr.on("data", (chunk) => stderr.push(chunk));
    child.on("error", (error) => resolve({ passed: false, error: error.message }));
    child.on("close", (code) => {
      clearTimeout(timer);
      resolve({ passed: code === 0, exit_code: code, stdout: Buffer.concat(stdout).toString("utf8").trim(), error: Buffer.concat(stderr).toString("utf8").trim() });
    });
  });
}

async function materialize(directory, files) {
  for (const [name, content] of Object.entries(safeFiles(files))) {
    const destination = path.join(directory, name);
    await mkdir(path.dirname(destination), { recursive: true });
    await writeFile(destination, content);
  }
}

function containsAny(source, patterns) { return patterns.some((pattern) => source.includes(pattern)); }

export async function evaluateFiles(condition, files, options = {}) {
  files = safeFiles(files);
  const task = options.task || await getTask(options.taskId || "inventory");
  const checks = [];
  let runtimeWorkspace;
  if (condition === "aml") {
    checks.push({ name: "one_main_aml", passed: Object.keys(files).length === 1 && typeof files["main.aml"] === "string" });
    const source = files["main.aml"] || "";
    for (const [name, pattern] of [
      ["fullstack_profile", "profile fullstack"],
      ["data_model", `data ${task.model}:`],
      ["resource", `resource ${task.resource}: ${task.model}:`],
      ...task.fields.map((field) => [`field_${field.name}`, `  ${field.name}: ${field.type}`]),
      ["list_action", "  list"], ["create_action", "  create"], ["update_action", "  update"], ["delete_action", "  delete"], ["page", "page "],
    ]) checks.push({ name, passed: source.includes(pattern) });
    const temporary = await mkdtemp(path.join(os.tmpdir(), "aml-benchmark-"));
    await materialize(temporary, files);
    const executable = process.env.AML_BIN || path.join(repository, "target", "release", process.platform === "win32" ? "aml.exe" : "aml");
    if (!existsSync(executable)) checks.push({ name: "aml_compiler", passed: false, detail: `not found: ${executable}` });
    else {
      const checked = await runCommand(executable, ["check", "main.aml", "--target", "fullstack", "--json"], temporary);
      checks.push({ name: "aml_check", ...checked });
      const built = await runCommand(executable, ["build", "main.aml", "--target", "fullstack", "-o", "generated"], temporary);
      checks.push({ name: "aml_build", ...built });
      if (built.passed) runtimeWorkspace = path.join(temporary, "generated");
    }
  } else if (condition === "typescript") {
    const combined = Object.values(files).join("\n");
    checks.push({ name: "package_manifest", passed: typeof files["package.json"] === "string" });
    let scripts = {};
    try { scripts = JSON.parse(files["package.json"] || "{}").scripts || {}; } catch {}
    checks.push({ name: "development_command", passed: typeof scripts.dev === "string" && scripts.dev.length > 0 });
    checks.push({ name: "production_build_command", passed: typeof scripts.build === "string" && scripts.build.length > 0 });
    checks.push({ name: "production_start_command", passed: typeof scripts.start === "string" && scripts.start.length > 0 });
    checks.push({ name: "browser_source", passed: Object.keys(files).some((name) => /(?:src\/.*\.(?:tsx|jsx)|index\.html)$/.test(name)) });
    checks.push({ name: "server_source", passed: Object.keys(files).some((name) => /server.*\.(?:ts|js|mjs)$/.test(name)) });
    checks.push({ name: "resource_api", passed: combined.includes(`/api/${task.resource}`) });
    for (const field of task.fields) checks.push({ name: `field_${field.name}`, passed: combined.includes(field.name) });
    checks.push({ name: "list_operation", passed: containsAny(combined, ["GET", ".get(", "method === \"GET\"", "method === 'GET'"]) });
    checks.push({ name: "create_operation", passed: containsAny(combined, ["POST", ".post(", "method === \"POST\"", "method === 'POST'"]) });
    checks.push({ name: "update_operation", passed: containsAny(combined, ["PUT", ".put(", "method === \"PUT\"", "method === 'PUT'"]) });
    checks.push({ name: "delete_operation", passed: containsAny(combined, ["DELETE", ".delete(", "method === \"DELETE\"", "method === 'DELETE'"]) });
    checks.push({ name: "validation", passed: containsAny(combined, ["unknown", "additionalProperties", "Object.keys", "strict("]) });
    checks.push({ name: "persistence", passed: containsAny(combined, ["writeFile", "writeFileSync", "sqlite", "better-sqlite", "lowdb"]) });
    checks.push({ name: "unique_ids", passed: containsAny(combined, ["randomUUID", "uuid", "crypto.randomUUID"]) });
    runtimeWorkspace = await mkdtemp(path.join(os.tmpdir(), "typescript-benchmark-"));
    await prepareRuntimeWorkspace(runtimeWorkspace, files);
  } else throw new Error(`unknown condition: ${condition}`);
  const structuralPassed = checks.every((check) => check.passed);
  if (options.execute && structuralPassed) {
    const runtime = await runtimeEvaluate(runtimeWorkspace, options.engine || "docker", task);
    return { level: "runtime", passed: runtime.passed, checks, runtime };
  }
  return { level: "structural", passed: structuralPassed, checks };
}

async function saveRound(runDirectory, condition, response, round, generationDurationMs = 0) {
  if (response.protocol !== PROTOCOL) throw new Error(`adapter must use protocol ${PROTOCOL}`);
  if (typeof response.model !== "string" || !response.model) throw new Error("adapter response needs model");
  const files = safeFiles(response.files);
  const conditionDirectory = path.join(runDirectory, condition);
  await writeFile(path.join(conditionDirectory, "response.json"), `${JSON.stringify({ files }, null, 2)}\n`);
  const usagePath = path.join(conditionDirectory, "usage.json");
  let recorded = { model: response.model, rounds: [] };
  if (existsSync(usagePath)) recorded = JSON.parse(await readFile(usagePath, "utf8"));
  if (recorded.model !== response.model) throw new Error("adapter model changed between repair rounds");
  recorded.rounds.push({ round, generation_duration_ms: generationDurationMs, ...usage(response.usage) });
  await writeFile(usagePath, `${JSON.stringify(recorded, null, 2)}\n`);
  return files;
}

async function recordEvaluationDuration(runDirectory, condition, durationMs) {
  const usagePath = path.join(runDirectory, condition, "usage.json");
  const recorded = JSON.parse(await readFile(usagePath, "utf8"));
  recorded.rounds[recorded.rounds.length - 1].evaluation_duration_ms = durationMs;
  await writeFile(usagePath, `${JSON.stringify(recorded, null, 2)}\n`);
}

export async function runBenchmark(runDirectory, adapter, maxRepairs = 2, execute = false, engine = "docker", taskId = "inventory") {
  if (execute && !(await containerEngineAvailable(engine))) throw new Error(`container engine '${engine}' is required for --execute`);
  if (conditions.some((condition) => existsSync(path.join(runDirectory, condition, "usage.json")))) throw new Error(`run directory already contains benchmark usage: ${runDirectory}`);
  const task = await getTask(taskId);
  const prompts = await prepare(runDirectory, taskId);
  for (const condition of conditions) {
    let previousFiles = null;
    let feedback = null;
    for (let round = 1; round <= maxRepairs + 1; round++) {
      const generationStarted = Date.now();
      const response = await invokeAdapter(adapter, { protocol: PROTOCOL, task, condition, round, prompt: prompts[condition], previous_files: previousFiles, feedback });
      previousFiles = await saveRound(runDirectory, condition, response, round, Date.now() - generationStarted);
      const evaluationStarted = Date.now();
      const evaluation = await evaluateFiles(condition, previousFiles, { execute, engine, task });
      await recordEvaluationDuration(runDirectory, condition, Date.now() - evaluationStarted);
      await writeFile(path.join(runDirectory, condition, "evaluation.json"), `${JSON.stringify(evaluation, null, 2)}\n`);
      if (evaluation.passed) break;
      const failed = [
        ...evaluation.checks.filter((check) => !check.passed),
        ...(evaluation.runtime?.checks || []).filter((check) => !check.passed),
      ];
      feedback = failed.map((check) => ({ name: check.name, detail: check.error || check.detail || check.stdout || "requirement not detected" }));
    }
  }
  return report(runDirectory);
}

export async function evaluate(runDirectory, execute = false, engine = "docker") {
  if (execute && !(await containerEngineAvailable(engine))) throw new Error(`container engine '${engine}' is required for --execute`);
  const metadata = JSON.parse(await readFile(path.join(runDirectory, "metadata.json"), "utf8"));
  const task = metadata.task_definition || await getTask(metadata.task);
  for (const condition of conditions) {
    const response = JSON.parse(await readFile(path.join(runDirectory, condition, "response.json"), "utf8"));
    const evaluation = await evaluateFiles(condition, response.files, { execute, engine, task });
    await writeFile(path.join(runDirectory, condition, "evaluation.json"), `${JSON.stringify(evaluation, null, 2)}\n`);
  }
}

export async function report(runDirectory) {
  const metadata = JSON.parse(await readFile(path.join(runDirectory, "metadata.json"), "utf8"));
  const result = { schema: "aml-ai-benchmark-report-2", task: metadata.task, scope: "provider-reported complete workflow", conditions: {} };
  for (const condition of conditions) {
    const recorded = JSON.parse(await readFile(path.join(runDirectory, condition, "usage.json"), "utf8"));
    const evaluation = JSON.parse(await readFile(path.join(runDirectory, condition, "evaluation.json"), "utf8"));
    const totals = recorded.rounds.reduce((sum, round) => ({ input_tokens: sum.input_tokens + round.input_tokens, output_tokens: sum.output_tokens + round.output_tokens, reasoning_tokens: sum.reasoning_tokens + round.reasoning_tokens, total_tokens: sum.total_tokens + round.total_tokens }), { input_tokens: 0, output_tokens: 0, reasoning_tokens: 0, total_tokens: 0 });
    result.conditions[condition] = {
      model: recorded.model,
      repair_rounds: Math.max(0, recorded.rounds.length - 1),
      generation_duration_ms: recorded.rounds.reduce((sum, round) => sum + (round.generation_duration_ms || 0), 0),
      evaluation_duration_ms: recorded.rounds.reduce((sum, round) => sum + (round.evaluation_duration_ms || 0), 0),
      totals,
      evaluation,
    };
  }
  const aml = result.conditions.aml;
  const typescript = result.conditions.typescript;
  if (aml.model.startsWith("mock/") || typescript.model.startsWith("mock/")) {
    result.comparison = { savings_percent: null, reason: "Mock-adapter usage is synthetic and cannot be reported as AML performance evidence." };
  } else if (aml.model === typescript.model && aml.evaluation.passed && typescript.evaluation.passed && aml.evaluation.level === typescript.evaluation.level) {
    result.comparison = { tokens_saved: typescript.totals.total_tokens - aml.totals.total_tokens, savings_percent: Number((((typescript.totals.total_tokens - aml.totals.total_tokens) / typescript.totals.total_tokens) * 100).toFixed(1)) };
  } else result.comparison = { savings_percent: null, reason: "Savings require the same model and both conditions passing the same evaluation level." };
  await writeFile(path.join(runDirectory, "report.json"), `${JSON.stringify(result, null, 2)}\n`);
  return result;
}

export async function runSuite(suiteDirectory, adapter, maxRepairs = 2, repetitions = 1, execute = false, engine = "docker", taskIds = null) {
  if (execute && !(await containerEngineAvailable(engine))) throw new Error(`container engine '${engine}' is required for --execute`);
  const catalog = await listTasks();
  const selected = taskIds ? taskIds.map((id) => {
    const task = catalog.find((candidate) => candidate.id === id);
    if (!task) throw new Error(`unknown task '${id}'`);
    return task;
  }) : catalog;
  const runs = [];
  for (const task of selected) {
    for (let repetition = 1; repetition <= repetitions; repetition++) {
      const directory = path.join(suiteDirectory, task.id, `run-${repetition}`);
      runs.push({ task: task.id, repetition, report: await runBenchmark(directory, adapter, maxRepairs, execute, engine, task.id) });
    }
  }
  const models = new Set(runs.flatMap((run) => conditions.map((condition) => run.report.conditions[condition].model)));
  const eligible = runs.filter((run) => run.report.comparison.savings_percent !== null);
  const totals = Object.fromEntries(conditions.map((condition) => [condition, runs.reduce((sum, run) => sum + run.report.conditions[condition].totals.total_tokens, 0)]));
  const passed = Object.fromEntries(conditions.map((condition) => [condition, runs.filter((run) => run.report.conditions[condition].evaluation.passed).length]));
  const firstPass = Object.fromEntries(conditions.map((condition) => [condition, runs.filter((run) => run.report.conditions[condition].evaluation.passed && run.report.conditions[condition].repair_rounds === 0).length]));
  const repairRounds = Object.fromEntries(conditions.map((condition) => [condition, runs.reduce((sum, run) => sum + run.report.conditions[condition].repair_rounds, 0)]));
  const generationDuration = Object.fromEntries(conditions.map((condition) => [condition, runs.reduce((sum, run) => sum + run.report.conditions[condition].generation_duration_ms, 0)]));
  const evaluationDuration = Object.fromEntries(conditions.map((condition) => [condition, runs.reduce((sum, run) => sum + run.report.conditions[condition].evaluation_duration_ms, 0)]));
  const byTask = Object.fromEntries(selected.map((task) => {
    const taskRuns = runs.filter((run) => run.task === task.id);
    const taskTotals = Object.fromEntries(conditions.map((condition) => [condition, taskRuns.reduce((sum, run) => sum + run.report.conditions[condition].totals.total_tokens, 0)]));
    const taskEligible = taskRuns.every((run) => run.report.comparison.savings_percent !== null);
    return [task.id, {
      runs: taskRuns.length,
      totals: taskTotals,
      passed: Object.fromEntries(conditions.map((condition) => [condition, taskRuns.filter((run) => run.report.conditions[condition].evaluation.passed).length])),
      comparison: taskEligible && taskTotals.typescript > 0
        ? { tokens_saved: taskTotals.typescript - taskTotals.aml, savings_percent: Number((((taskTotals.typescript - taskTotals.aml) / taskTotals.typescript) * 100).toFixed(1)) }
        : { savings_percent: null },
    }];
  }));
  const result = {
    schema: "aml-ai-benchmark-suite-report-1",
    tasks: selected.map((task) => task.id),
    repetitions,
    scheduled_runs: runs.length,
    evaluation_level: execute ? "runtime" : "structural",
    totals,
    average_tokens: Object.fromEntries(conditions.map((condition) => [condition, Number((totals[condition] / runs.length).toFixed(1))])),
    passed,
    pass_rate_percent: Object.fromEntries(conditions.map((condition) => [condition, Number(((passed[condition] / runs.length) * 100).toFixed(1))])),
    first_pass: firstPass,
    first_pass_rate_percent: Object.fromEntries(conditions.map((condition) => [condition, Number(((firstPass[condition] / runs.length) * 100).toFixed(1))])),
    repair_rounds: repairRounds,
    average_repair_rounds: Object.fromEntries(conditions.map((condition) => [condition, Number((repairRounds[condition] / runs.length).toFixed(2))])),
    generation_duration_ms: generationDuration,
    average_generation_duration_ms: Object.fromEntries(conditions.map((condition) => [condition, Number((generationDuration[condition] / runs.length).toFixed(1))])),
    evaluation_duration_ms: evaluationDuration,
    average_evaluation_duration_ms: Object.fromEntries(conditions.map((condition) => [condition, Number((evaluationDuration[condition] / runs.length).toFixed(1))])),
    by_task: byTask,
    runs,
  };
  if (eligible.length === runs.length && models.size === 1 && runs.length > 0 && totals.typescript > 0) {
    result.comparison = {
      model: [...models][0],
      tokens_saved: totals.typescript - totals.aml,
      savings_percent: Number((((totals.typescript - totals.aml) / totals.typescript) * 100).toFixed(1)),
    };
  } else {
    result.comparison = { savings_percent: null, reason: "Suite savings require every scheduled run to use one non-mock model and both conditions to pass the same evaluation level." };
  }
  await mkdir(suiteDirectory, { recursive: true });
  await writeFile(path.join(suiteDirectory, "suite-report.json"), `${JSON.stringify(result, null, 2)}\n`);
  return result;
}

async function cli() {
  const [command, ...args] = process.argv.slice(2);
  const value = (name, fallback) => { const index = args.indexOf(name); return index < 0 ? fallback : args[index + 1]; };
  const runDirectory = path.resolve(value("--run-dir", "benchmark-run"));
  if (command === "list") console.log(JSON.stringify(await listTasks(), null, 2));
  else if (command === "prepare") await prepare(runDirectory, value("--task", "inventory"));
  else if (command === "evaluate") await evaluate(runDirectory, args.includes("--execute"), value("--engine", "docker"));
  else if (command === "report") console.log(JSON.stringify(await report(runDirectory), null, 2));
  else if (command === "run" || command === "suite") {
    const adapter = value("--adapter");
    if (!adapter) throw new Error(`${command} requires --adapter EXECUTABLE`);
    const maxRepairs = Number(value("--max-repairs", "2"));
    if (!Number.isInteger(maxRepairs) || maxRepairs < 0 || maxRepairs > 10) throw new Error("--max-repairs must be 0-10");
    if (command === "run") console.log(JSON.stringify(await runBenchmark(runDirectory, path.resolve(adapter), maxRepairs, args.includes("--execute"), value("--engine", "docker"), value("--task", "inventory")), null, 2));
    else {
      const repetitions = Number(value("--repetitions", "1"));
      if (!Number.isInteger(repetitions) || repetitions < 1 || repetitions > 10) throw new Error("--repetitions must be 1-10");
      const selected = value("--tasks", "").split(",").filter(Boolean);
      console.log(JSON.stringify(await runSuite(runDirectory, path.resolve(adapter), maxRepairs, repetitions, args.includes("--execute"), value("--engine", "docker"), selected.length ? selected : null), null, 2));
    }
  } else throw new Error("usage: benchmark.mjs <list|prepare|evaluate|report|run|suite> --run-dir DIR [--task ID] [--tasks ID,...] [--adapter EXECUTABLE] [--max-repairs N] [--repetitions N] [--execute] [--engine docker|podman]");
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) cli().catch((error) => { console.error(`benchmark: ${error.message}`); process.exit(1); });

