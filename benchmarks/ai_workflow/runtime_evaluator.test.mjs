import assert from "node:assert/strict";
import test from "node:test";
import { containerEngineAvailable, runtimeEvaluate, sandboxArguments, sanitizedManifest } from "./runtime_evaluator.mjs";

test("sandbox denies network and drops privileges", () => {
  const args = sandboxArguments("/tmp/project");
  assert.deepEqual(args.slice(0, 2), ["--network", "none"]);
  assert.ok(args.includes("--read-only"));
  assert.ok(args.includes("--cap-drop"));
  assert.ok(args.includes("no-new-privileges"));
  assert.ok(args.includes("--pids-limit"));
});

test("manifest strips scripts and accepts allowlisted packages", () => {
  const output = JSON.parse(sanitizedManifest(JSON.stringify({ scripts: { build: "curl bad" }, dependencies: { react: "^19.0.0", express: "latest" } })));
  assert.equal(output.scripts, undefined);
  assert.equal(output.dependencies.react, "^19.0.0");
});

test("manifest rejects package URLs and unapproved dependencies", () => {
  assert.throws(() => sanitizedManifest(JSON.stringify({ dependencies: { react: "https://example.com/a.tgz" } })), /version/);
  assert.throws(() => sanitizedManifest(JSON.stringify({ dependencies: { "malicious-package": "1.0.0" } })), /rejects dependency/);
});

test("container engine names are restricted", async () => {
  assert.equal(await containerEngineAvailable("node"), false);
  const result = await runtimeEvaluate("/tmp/project", "node");
  assert.equal(result.passed, false);
  assert.match(result.checks[0].error, /docker or podman/);
});
