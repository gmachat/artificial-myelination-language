import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";

const expected = [
  "AML Sprint Demo",
  "Design AI workflow",
  "assigned",
  "5",
  "Build compiler",
  "unassigned",
  "0",
  "5",
  "5",
].join("\n") + "\n";

const result = spawnSync(process.execPath, ["build/app.js"], {
  cwd: new URL(".", import.meta.url),
  encoding: "utf8",
});

assert.equal(result.status, 0, result.stderr);
assert.equal(result.stdout, expected);
console.log("PASS: generated JavaScript matches the AML behavior");
