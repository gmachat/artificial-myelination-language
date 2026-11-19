#!/usr/bin/env node
import { spawn } from "node:child_process";
import { createInterface } from "node:readline";
import path from "node:path";

const root = path.resolve(import.meta.dirname, "..");
const executable = process.env.AML_BIN || path.join(root, "target", "release", process.platform === "win32" ? "aml.exe" : "aml");
const child = spawn(executable, ["mcp", "--project", root], { stdio: ["pipe", "pipe", "pipe"] });
const pending = new Map();
let nextId = 1;
let stderr = "";
child.stderr.on("data", (chunk) => { stderr += chunk; });

const lines = createInterface({ input: child.stdout });
lines.on("line", (line) => {
  let message;
  try { message = JSON.parse(line); }
  catch (error) { for (const item of pending.values()) item.reject(new Error(`invalid JSON-RPC: ${error.message}`)); return; }
  const item = pending.get(message.id);
  if (item) { pending.delete(message.id); message.error ? item.reject(new Error(message.error.message)) : item.resolve(message.result); }
});

function call(method, params = {}) {
  const id = nextId++;
  child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", id, method, params })}\n`);
  return new Promise((resolve, reject) => {
    const timer = setTimeout(() => { pending.delete(id); reject(new Error(`timeout: ${method}`)); }, 10000);
    pending.set(id, {
      resolve: (value) => { clearTimeout(timer); resolve(value); },
      reject: (error) => { clearTimeout(timer); reject(error); },
    });
  });
}

try {
  const discovery = await call("server/discover", { _meta: { "io.modelcontextprotocol/protocolVersion": "2026-07-28", "io.modelcontextprotocol/clientCapabilities": {} } });
  if (!discovery.supportedVersions.includes("2026-07-28")) throw new Error("modern discovery failed");
  const initialized = await call("initialize", { protocolVersion: "2025-11-25", capabilities: {}, clientInfo: { name: "aml-smoke", version: "1" } });
  if (initialized.protocolVersion !== "2025-11-25") throw new Error("legacy initialize failed");
  child.stdin.write(`${JSON.stringify({ jsonrpc: "2.0", method: "notifications/initialized", params: {} })}\n`);
  const catalog = await call("tools/list");
  const names = catalog.tools.map((tool) => tool.name);
  for (const required of ["aml_project_context", "aml_check", "aml_format", "aml_build", "aml_diagnose", "aml_repair_context", "aml_test", "aml_doctor", "aml_targets"]) {
    if (!names.includes(required)) throw new Error(`missing tool: ${required}`);
  }
  const checked = await call("tools/call", { name: "aml_check", arguments: { path: "examples/hello.aml", target: "javascript" } });
  if (checked.isError || !checked.content?.[0]?.text.includes('"valid": true')) throw new Error("aml_check failed");
  const tested = await call("tools/call", { name: "aml_test", arguments: { directory: "examples/testing/tests", target: "javascript" } });
  if (tested.isError || !tested.content?.[0]?.text.includes('"failed": 0')) throw new Error("aml_test failed");
  console.log(`AML MCP smoke test passed (${names.length} tools)`);
} finally {
  child.stdin.end();
  child.kill();
}

child.on("error", (error) => { throw error; });
if (stderr.trim()) console.error(stderr.trim());

