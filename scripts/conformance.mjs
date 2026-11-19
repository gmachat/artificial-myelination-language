#!/usr/bin/env node
import { accessSync, constants } from "node:fs";
import { spawnSync } from "node:child_process";
import path from "node:path";

const root = path.resolve(import.meta.dirname, "..");
const executable = process.env.AML_BIN || path.join(root, "target", "release", process.platform === "win32" ? "aml.exe" : "aml");
const fixture = path.join(root, "examples", "hello.aml");
const targets = [
  ["javascript", "node", ["--version"]], ["python", "python3", ["--version"]],
  ["ruby", "ruby", ["--version"]], ["go", "go", ["version"]],
  ["rust", "rustc", ["--version"]], ["c", "cc", ["--version"]],
  ["java", "javac", ["--version"]], ["csharp", "dotnet", ["--version"]],
  ["swift", "swift", ["-e", "print(\"ready\")"]],
];

try { accessSync(executable, constants.X_OK); }
catch { console.error(`AML binary not found: ${executable}\nRun: cargo build --release`); process.exit(2); }

const results = [];
for (const [target, runtime, probeArgs] of targets) {
  const probe = spawnSync(runtime, probeArgs, { encoding: "utf8", timeout: 30000 });
  if (probe.error?.code === "ENOENT" || probe.status !== 0) {
    const detail = (probe.stderr || probe.error?.message || "not installed").trim().split("\n")[0];
    results.push({ target, status: "skipped", reason: `${runtime} unavailable: ${detail}` });
    continue;
  }
  const run = spawnSync(executable, ["run", fixture, "--target", target], { encoding: "utf8", timeout: 120000 });
  const output = run.stdout.trim();
  const error = run.stderr.trim();
  const brokenToolchain = /SDK is not supported by the compiler|unable to locate a Java Runtime/.test(error);
  results.push(run.status === 0 && output === "Hello, AML!"
    ? { target, status: "passed" }
    : brokenToolchain
      ? { target, status: "skipped", reason: `${runtime} toolchain is installed but unusable: ${error.split("\n")[0]}` }
      : { target, status: "failed", exitCode: run.status, output, error });
}
const summary = {
  passed: results.filter((item) => item.status === "passed").length,
  failed: results.filter((item) => item.status === "failed").length,
  skipped: results.filter((item) => item.status === "skipped").length,
};
console.log(JSON.stringify({ schema: "aml-conformance-1", summary, results }, null, 2));
process.exitCode = summary.failed ? 1 : 0;

