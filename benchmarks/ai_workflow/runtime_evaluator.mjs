import { spawn } from "node:child_process";
import { mkdir, readFile, writeFile } from "node:fs/promises";
import { existsSync } from "node:fs";
import path from "node:path";

const ALLOWED_PACKAGES = new Set([
  "@types/cors", "@types/express", "@types/node", "@types/react",
  "@types/react-dom", "@types/uuid", "@vitejs/plugin-react", "cors", "express",
  "lowdb", "nanoid", "react", "react-dom", "tsx", "typescript", "uuid", "vite", "zod",
]);
const VERSION = /^(?:[~^]?\d+\.\d+\.\d+(?:-[0-9A-Za-z.-]+)?|latest)$/;
const IMAGE = process.env.AML_BENCHMARK_NODE_IMAGE || "node:22-bookworm-slim";

export function sandboxArguments(workspace, network = "none") {
  return [
    "--network", network,
    "--memory", "768m",
    "--cpus", "1",
    "--pids-limit", "128",
    "--cap-drop", "ALL",
    "--security-opt", "no-new-privileges",
    "--read-only",
    "--tmpfs", "/tmp:rw,noexec,nosuid,size=64m",
    "--mount", `type=bind,source=${workspace},target=/workspace`,
    "--workdir", "/workspace",
  ];
}

export function sanitizedManifest(source) {
  const original = JSON.parse(source);
  const clean = { private: true, type: "module", dependencies: {}, devDependencies: {} };
  for (const field of ["dependencies", "devDependencies"]) {
    const values = original[field] || {};
    if (!values || typeof values !== "object" || Array.isArray(values)) throw new Error(`${field} must be an object`);
    for (const [name, version] of Object.entries(values)) {
      if (!ALLOWED_PACKAGES.has(name)) throw new Error(`runtime evaluator rejects dependency '${name}'`);
      if (typeof version !== "string" || !VERSION.test(version)) throw new Error(`runtime evaluator rejects dependency version '${name}@${version}'`);
      clean[field][name] = version;
    }
  }
  return `${JSON.stringify(clean, null, 2)}\n`;
}

function command(executable, args, options = {}) {
  return new Promise((resolve) => {
    const { timeoutMs = 180_000, ...spawnOptions } = options;
    const child = spawn(executable, args, { stdio: ["ignore", "pipe", "pipe"], ...spawnOptions });
    const stdout = [];
    const stderr = [];
    let timedOut = false;
    let settled = false;
    const finish = (result) => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      resolve(result);
    };
    const timer = setTimeout(() => { timedOut = true; child.kill("SIGKILL"); }, timeoutMs);
    child.stdout.on("data", (chunk) => stdout.push(chunk));
    child.stderr.on("data", (chunk) => stderr.push(chunk));
    child.on("error", (error) => finish({ passed: false, error: error.message, timed_out: timedOut }));
    child.on("close", (code) => {
      finish({ passed: code === 0 && !timedOut, exit_code: code, timed_out: timedOut, stdout: Buffer.concat(stdout).toString("utf8").trim(), error: Buffer.concat(stderr).toString("utf8").trim() });
    });
  });
}

export async function containerEngineAvailable(engine = "docker") {
  if (!["docker", "podman"].includes(engine)) return false;
  return (await command(engine, ["version"], { timeoutMs: 20_000 })).passed;
}

function serverFile(workspace) {
  const candidates = ["server.mjs", "server.js", "server.ts", "src/server.mjs", "src/server.js", "src/server.ts"];
  return candidates.find((name) => existsSync(path.join(workspace, name)));
}

function serverCommand(file) {
  return file.endsWith(".ts") ? ["./node_modules/.bin/tsx", file] : ["node", file];
}

function payload(task, key) { return Object.fromEntries(task.fields.map((field) => [field.name, field[key]])); }

function invalidValue(type) {
  return { Text: 42, Int: "not-an-integer", Float: "not-a-number", Bool: "not-a-boolean", Time: "not-a-time" }[type];
}

function runtimeConfig(task) {
  const first = payload(task, "first");
  const missing = { ...first };
  delete missing[task.fields[0].name];
  return {
    endpoint: `/api/${task.resource}`,
    first,
    second: payload(task, "second"),
    updated: payload(task, "updated"),
    invalid: [
      ["unknown", { ...first, unknown_field: "reject" }],
      ["missing", missing],
      ["wrong_type", { ...first, [task.fields[0].name]: invalidValue(task.fields[0].type) }],
      ...(task.fields.some((field) => field.type === "Text")
        ? [["empty_text", { ...first, [task.fields.find((field) => field.type === "Text").name]: "  " }]]
        : []),
    ],
  };
}

function createTest(task) { return String.raw`
const config=${JSON.stringify(runtimeConfig(task))};
const base='http://127.0.0.1:3000';
const sleep=ms=>new Promise(r=>setTimeout(r,ms));
let response;
for(let i=0;i<100;i++){try{response=await fetch(base+config.endpoint);if(response.ok)break}catch{}await sleep(100)}
if(!response?.ok)throw new Error('server did not become ready');
const initial=await response.json();
const list=value=>Array.isArray(value)?value:(value.data||value.products||[]);
if(!Array.isArray(list(initial)))throw new Error('list response has no array');
const create=body=>fetch(base+config.endpoint,{method:'POST',headers:{'content-type':'application/json'},body:JSON.stringify(body)});
response=await create(config.first);
if(response.status<200||response.status>=300)throw new Error('valid create failed: '+response.status);
const createdRaw=await response.json();const created=createdRaw.data||createdRaw.product||createdRaw;
if(!created.id)throw new Error('created record has no id');
response=await create(config.second);const secondRaw=await response.json();const second=secondRaw.data||secondRaw.product||secondRaw;
if(!response.ok||!second.id||String(second.id)===String(created.id))throw new Error('unique ids were not produced');
for(const [label,value] of config.invalid){response=await create(value);if(response.status<400)throw new Error(label+' payload was accepted')}
response=await fetch(base+config.endpoint+'/'+encodeURIComponent(created.id),{method:'PUT',headers:{'content-type':'application/json'},body:JSON.stringify(config.updated)});
if(!response.ok)throw new Error('update failed: '+response.status);
const updatedRaw=await response.json();const updated=updatedRaw.data||updatedRaw;
for(const [key,value] of Object.entries(config.updated)){if(updated[key]!==value)throw new Error('update did not return '+key)}
response=await fetch(base+config.endpoint);const listedRaw=await response.json();const listed=list(listedRaw);const listedItem=listed.find(item=>String(item.id)===String(created.id));
if(!listedItem)throw new Error('updated record was not listed');
for(const [key,value] of Object.entries(config.updated)){if(listedItem[key]!==value)throw new Error('update did not persist '+key)}
process.stdout.write(String(created.id));
`; }

function persistenceTest(task) { return String.raw`
const config=${JSON.stringify(runtimeConfig(task))};
const base='http://127.0.0.1:3000';const id=process.env.TEST_ID;
const sleep=ms=>new Promise(r=>setTimeout(r,ms));let response;
for(let i=0;i<100;i++){try{response=await fetch(base+config.endpoint);if(response.ok)break}catch{}await sleep(100)}
if(!response?.ok)throw new Error('server did not restart');
const raw=await response.json();const items=Array.isArray(raw)?raw:(raw.data||raw.products||[]);
const persisted=items.find(item=>String(item.id)===id);if(!persisted)throw new Error('record did not persist across restart');
for(const [key,value] of Object.entries(config.updated)){if(persisted[key]!==value)throw new Error('updated '+key+' did not persist across restart')}
response=await fetch(base+config.endpoint+'/'+encodeURIComponent(id),{method:'DELETE'});if(response.status<200||response.status>=300)throw new Error('delete failed');
response=await fetch(base+config.endpoint);const afterRaw=await response.json();const after=Array.isArray(afterRaw)?afterRaw:(afterRaw.data||afterRaw.products||[]);
if(after.some(item=>String(item.id)===id))throw new Error('deleted record still listed');
response=await fetch(base+'/');if(!response.ok||!(await response.text()).toLowerCase().includes('<'))throw new Error('production browser page not served');
console.log('runtime behavior passed');
`; }

async function start(engine, workspace, name, file) {
  return command(engine, ["run", "--detach", "--name", name, ...sandboxArguments(workspace), "--env", "PORT=3000", IMAGE, ...serverCommand(file)]);
}

export async function runtimeEvaluate(workspace, engine = "docker", task = { resource: "products", fields: [
  { name: "name", type: "Text", first: "Widget", second: "Gadget", updated: "Widget Pro" },
  { name: "quantity", type: "Int", first: 3, second: 4, updated: 8 },
  { name: "active", type: "Bool", first: true, second: false, updated: false },
] }) {
  const checks = [];
  if (!["docker", "podman"].includes(engine)) return { level: "runtime", passed: false, checks: [{ name: "container_engine", passed: false, error: "engine must be docker or podman" }] };
  const probe = await command(engine, ["version"], { timeoutMs: 20_000 });
  if (!probe.passed) return { level: "runtime", passed: false, checks: [{ name: "container_engine", passed: false, error: probe.error || `${engine} is unavailable` }] };
  const manifestPath = path.join(workspace, "package.json");
  if (!existsSync(manifestPath)) return { level: "runtime", passed: false, checks: [{ name: "package_manifest", passed: false, error: "package.json is required" }] };
  try { await writeFile(manifestPath, sanitizedManifest(await readFile(manifestPath, "utf8"))); }
  catch (error) { return { level: "runtime", passed: false, checks: [{ name: "dependency_policy", passed: false, error: error.message }] }; }

  // Network is permitted only for allowlisted package download. Lifecycle scripts are disabled.
  const install = await command(engine, ["run", "--rm", ...sandboxArguments(workspace, "bridge"), "--read-only=false", "--env", "npm_config_cache=/tmp/npm-cache", IMAGE, "npm", "install", "--ignore-scripts", "--no-audit", "--no-fund"], { timeoutMs: 300_000 });
  checks.push({ name: "dependency_install", ...install });
  if (!install.passed) return { level: "runtime", passed: false, checks };

  const build = await command(engine, ["run", "--rm", ...sandboxArguments(workspace), IMAGE, "./node_modules/.bin/vite", "build"], { timeoutMs: 180_000 });
  checks.push({ name: "production_build", ...build });
  if (!build.passed) return { level: "runtime", passed: false, checks };
  const file = serverFile(workspace);
  if (!file) { checks.push({ name: "server_entry", passed: false, error: "server.mjs, server.js, or server.ts was not found" }); return { level: "runtime", passed: false, checks }; }

  const nonce = `${process.pid}-${Date.now()}`;
  const firstName = `aml-eval-a-${nonce}`;
  const secondName = `aml-eval-b-${nonce}`;
  try {
    const first = await start(engine, workspace, firstName, file);
    checks.push({ name: "server_start", ...first });
    if (!first.passed) return { level: "runtime", passed: false, checks };
    const created = await command(engine, ["exec", firstName, "node", "--input-type=module", "-e", createTest(task)]);
    checks.push({ name: "create_and_validation", ...created });
    if (!created.passed) return { level: "runtime", passed: false, checks };
    await command(engine, ["rm", "-f", firstName]);
    const second = await start(engine, workspace, secondName, file);
    checks.push({ name: "server_restart", ...second });
    if (!second.passed) return { level: "runtime", passed: false, checks };
    const persistence = await command(engine, ["exec", "--env", `TEST_ID=${created.stdout}`, secondName, "node", "--input-type=module", "-e", persistenceTest(task)]);
    checks.push({ name: "persistence_delete_and_browser", ...persistence });
  } finally {
    await command(engine, ["rm", "-f", firstName], { timeoutMs: 20_000 });
    await command(engine, ["rm", "-f", secondName], { timeoutMs: 20_000 });
  }
  return { level: "runtime", image: IMAGE, passed: checks.every((check) => check.passed), checks };
}

export async function prepareRuntimeWorkspace(directory, files) {
  await mkdir(directory, { recursive: true });
  for (const [name, content] of Object.entries(files)) {
    const destination = path.join(directory, name);
    await mkdir(path.dirname(destination), { recursive: true });
    await writeFile(destination, content);
  }
}

