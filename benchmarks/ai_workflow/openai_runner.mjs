#!/usr/bin/env node
import { spawn } from "node:child_process";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const configPath = path.join(here, ".env.openai");
const allowed = new Set(["OPENAI_API_KEY", "OPENAI_MODEL", "OPENAI_MAX_OUTPUT_TOKENS"]);

export function parseEnvironment(source) {
  const values = {};
  for (const [index, original] of source.split(/\r?\n/).entries()) {
    const line = original.trim();
    if (!line || line.startsWith("#")) continue;
    const separator = line.indexOf("=");
    if (separator < 1) throw new Error(`invalid setting on line ${index + 1}`);
    const name = line.slice(0, separator).trim();
    let value = line.slice(separator + 1).trim();
    if (!allowed.has(name)) throw new Error(`unsupported setting '${name}'`);
    if ((value.startsWith('"') && value.endsWith('"')) || (value.startsWith("'") && value.endsWith("'"))) value = value.slice(1, -1);
    if (!value) throw new Error(`${name} cannot be empty`);
    values[name] = value;
  }
  return values;
}

async function configuration() {
  let source;
  try { source = await readFile(configPath, "utf8"); }
  catch (error) {
    if (error.code === "ENOENT") throw new Error(`missing ${configPath}; copy .env.openai.example to .env.openai`);
    throw error;
  }
  const fileValues = parseEnvironment(source);
  const values = Object.fromEntries([...allowed].map((name) => [name, process.env[name] || fileValues[name]]));
  if (!values.OPENAI_API_KEY || values.OPENAI_API_KEY === "paste_your_openai_api_key_here") throw new Error(`replace the OPENAI_API_KEY placeholder in ${configPath}`);
  if (!values.OPENAI_MODEL) throw new Error(`set OPENAI_MODEL in ${configPath}`);
  if (values.OPENAI_MAX_OUTPUT_TOKENS && (!/^\d+$/.test(values.OPENAI_MAX_OUTPUT_TOKENS) || Number(values.OPENAI_MAX_OUTPUT_TOKENS) < 1000)) throw new Error("OPENAI_MAX_OUTPUT_TOKENS must be an integer of at least 1000");
  return values;
}

async function main() {
  const [command, ...args] = process.argv.slice(2);
  if (!command) throw new Error("usage: openai_runner.mjs <check|run|suite> [benchmark options]");
  const values = await configuration();
  if (command === "check") {
    console.log(`OpenAI benchmark configuration is ready for model ${values.OPENAI_MODEL}. No API call was made.`);
    return;
  }
  if (!["run", "suite"].includes(command)) throw new Error("command must be check, run, or suite");
  if (args.includes("--adapter")) throw new Error("openai_runner selects the OpenAI adapter automatically");
  const child = spawn(process.execPath, [path.join(here, "benchmark.mjs"), command, "--adapter", path.join(here, "openai_adapter.mjs"), ...args], {
    stdio: "inherit",
    env: { ...process.env, ...values },
  });
  const code = await new Promise((resolve, reject) => {
    child.on("error", reject);
    child.on("close", (status, signal) => resolve(signal ? 1 : status ?? 1));
  });
  process.exitCode = code;
}

if (process.argv[1] && import.meta.url === pathToFileURL(process.argv[1]).href) main().catch((error) => { console.error(`openai benchmark: ${error.message}`); process.exit(1); });
