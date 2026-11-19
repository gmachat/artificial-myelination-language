"""Full-stack React + Node application generation for AML."""

from __future__ import annotations

from dataclasses import dataclass
import json
import re

from .compiler import CompileError, NAME
from .web import REACT_VERSION, VITE_REACT_VERSION, VITE_VERSION

PG_VERSION = "8.23.1"
PERSISTENT_TYPES = {"Text", "Int", "Float", "Bool", "Time"}

@dataclass
class Model:
    name: str
    fields: list[tuple[str, str]]


@dataclass
class Resource:
    name: str
    model: str
    actions: list[str]


@dataclass
class FullstackSpec:
    app: str
    page: str
    path: str
    storage: str
    models: dict[str, Model]
    resources: list[Resource]


def _significant(source: str) -> list[tuple[int, int, str]]:
    result: list[tuple[int, int, str]] = []
    for number, raw in enumerate(source.splitlines(), start=1):
        if "\t" in raw:
            raise CompileError("tabs are not allowed; use two spaces", number)
        if not raw.strip() or raw.lstrip().startswith("#"):
            continue
        leading = len(raw) - len(raw.lstrip(" "))
        if leading % 2:
            raise CompileError("indentation must use multiples of two spaces", number)
        result.append((number, leading // 2, raw.strip()))
    return result


def parse_fullstack(source: str) -> FullstackSpec:
    lines = _significant(source)
    if len(lines) < 2:
        raise CompileError("full-stack source requires app and profile declarations", 1)
    app = re.fullmatch(rf"app\s+({NAME})", lines[0][2])
    if not app or lines[0][1]:
        raise CompileError("the first declaration must be 'app <name>'", lines[0][0])
    if lines[1][1] or lines[1][2] != "profile fullstack":
        raise CompileError("full-stack target requires 'profile fullstack'", lines[1][0])

    models: dict[str, Model] = {}
    resources: list[Resource] = []
    storage = "json"
    storage_seen = False
    page_name: str | None = None
    page_path: str | None = None
    page_resources: list[str] = []
    index = 2
    while index < len(lines):
        number, indent, text = lines[index]
        if indent:
            raise CompileError("unexpected top-level indentation", number)
        storage_match = re.fullmatch(r"storage\s+(json|postgres)", text)
        if storage_match:
            if storage_seen:
                raise CompileError("only one storage declaration is allowed", number)
            storage = storage_match.group(1)
            storage_seen = True
            index += 1
            continue
        data_match = re.fullmatch(rf"data\s+({NAME}):", text)
        if data_match:
            model_name = data_match.group(1)
            if not model_name[:1].isupper():
                raise CompileError("data names must begin with an uppercase letter", number)
            if model_name in models:
                raise CompileError(f"duplicate data declaration '{model_name}'", number)
            fields: list[tuple[str, str]] = []
            index += 1
            while index < len(lines) and lines[index][1] > 0:
                field_number, field_indent, field_text = lines[index]
                if field_indent != 1:
                    raise CompileError("data fields use one indentation level", field_number)
                field = re.fullmatch(rf"({NAME})\s*:\s*({NAME})", field_text)
                if not field:
                    raise CompileError("data fields use 'name: Type'", field_number)
                field_name, field_type = field.groups()
                if field_name == "id":
                    raise CompileError("id is generated automatically", field_number)
                if any(existing == field_name for existing, _ in fields):
                    raise CompileError(f"duplicate field '{field_name}'", field_number)
                if field_type not in PERSISTENT_TYPES:
                    choices = "Bool, Float, Int, Text, Time"
                    raise CompileError(
                        f"persistent field type must be one of: {choices}", field_number
                    )
                fields.append((field_name, field_type))
                index += 1
            if not fields:
                raise CompileError("data declaration requires at least one field", number)
            models[model_name] = Model(model_name, fields)
            continue

        resource_match = re.fullmatch(rf"resource\s+({NAME})\s*:\s*({NAME}):", text)
        if resource_match:
            resource_name, model_name = resource_match.groups()
            if any(item.name == resource_name for item in resources):
                raise CompileError(f"duplicate resource '{resource_name}'", number)
            actions: list[str] = []
            index += 1
            while index < len(lines) and lines[index][1] > 0:
                action_number, action_indent, action = lines[index]
                if action_indent != 1:
                    raise CompileError("resource actions use one indentation level", action_number)
                if action not in {"list", "create", "update", "delete"}:
                    raise CompileError(
                        "resource action must be list, create, update, or delete", action_number
                    )
                if action in actions:
                    raise CompileError(f"duplicate resource action '{action}'", action_number)
                actions.append(action)
                index += 1
            if not actions:
                raise CompileError("resource requires at least one action", number)
            resources.append(Resource(resource_name, model_name, actions))
            continue

        page_match = re.fullmatch(rf'page\s+({NAME})\s+at\s+"([^"\s]+)":', text)
        if page_match:
            if page_name:
                raise CompileError("fullstack 0.6 supports one page", number)
            page_name, page_path = page_match.groups()
            index += 1
            while index < len(lines) and lines[index][1] > 0:
                item_number, item_indent, item = lines[index]
                if item_indent != 1:
                    raise CompileError("page entries use one indentation level", item_number)
                page_resource = re.fullmatch(rf"resource\s+({NAME})", item)
                if not page_resource:
                    raise CompileError("page entries use 'resource <name>'", item_number)
                page_resources.append(page_resource.group(1))
                index += 1
            continue
        raise CompileError(f"unknown full-stack declaration: {text}", number)

    for resource in resources:
        if resource.model not in models:
            raise CompileError(
                f"resource '{resource.name}' references unknown data '{resource.model}'", 1
            )
    known_resources = {resource.name for resource in resources}
    for name in page_resources:
        if name not in known_resources:
            raise CompileError(f"page references unknown resource '{name}'", 1)
    if not resources:
        raise CompileError("full-stack application requires a resource", lines[-1][0])
    if not page_name or page_path is None:
        raise CompileError("full-stack application requires a page", lines[-1][0])
    selected = [resource for resource in resources if resource.name in page_resources]
    if not selected:
        raise CompileError("page requires at least one resource", lines[-1][0])
    return FullstackSpec(app.group(1), page_name, page_path, storage, models, selected)


def _resource_config(spec: FullstackSpec) -> list[dict[str, object]]:
    result: list[dict[str, object]] = []
    for resource in spec.resources:
        model = spec.models[resource.model]
        result.append(
            {
                "name": resource.name,
                "label": resource.name.replace("_", " ").title(),
                "actions": resource.actions,
                "fields": [
                    {
                        "name": name,
                        "label": name.replace("_", " ").title(),
                        "type": type_name,
                    }
                    for name, type_name in model.fields
                ],
            }
        )
    return result


def _server(config_json: str) -> str:
    return f'''import {{ randomUUID }} from "node:crypto";
import {{ createServer }} from "node:http";
import {{ readFile, rename, writeFile }} from "node:fs/promises";
import path from "node:path";
import {{ fileURLToPath }} from "node:url";

const directory = path.dirname(fileURLToPath(import.meta.url));
const dataFile = process.env.AML_DATA_FILE || path.join(directory, "data", "store.json");
const publicDirectory = path.join(directory, "dist");
const resources = {config_json};
const resourceMap = new Map(resources.map(resource => [resource.name, resource]));
let mutationQueue = Promise.resolve();

function send(response, status, value) {{
  const body = JSON.stringify(value);
  response.writeHead(status, {{ "content-type": "application/json; charset=utf-8" }});
  response.end(body);
}}

async function store() {{
  try {{
    return JSON.parse(await readFile(dataFile, "utf8"));
  }} catch (error) {{
    if (error.code === "ENOENT") return Object.fromEntries(resources.map(item => [item.name, []]));
    throw error;
  }}
}}

function mutate(operation) {{
  const next = mutationQueue.then(async () => {{
    const value = await store();
    const result = await operation(value);
    const temporary = `${{dataFile}}.tmp`;
    await writeFile(temporary, JSON.stringify(value, null, 2) + "\\n", "utf8");
    await rename(temporary, dataFile);
    return result;
  }});
  mutationQueue = next.then(() => undefined, () => undefined);
  return next;
}}

function valid(value, type) {{
  if (type === "Text") return typeof value === "string";
  if (type === "Int") return Number.isInteger(value);
  if (type === "Float") return typeof value === "number" && Number.isFinite(value);
  if (type === "Bool") return typeof value === "boolean";
  if (type === "Time") return typeof value === "string" && Number.isFinite(Date.parse(value));
  return false;
}}

function validate(resource, payload) {{
  const allowed = new Set(resource.fields.map(field => field.name));
  for (const key of Object.keys(payload)) {{
    if (!allowed.has(key)) throw new Error(`unknown field: ${{key}}`);
  }}
  const result = {{}};
  for (const field of resource.fields) {{
    const value = payload[field.name];
    if (!valid(value, field.type)) {{
      throw new Error(`${{field.name}} must be ${{field.type}}`);
    }}
    result[field.name] = value;
  }}
  return result;
}}

async function body(request) {{
  const chunks = [];
  let size = 0;
  for await (const chunk of request) {{
    size += chunk.length;
    if (size > 1_000_000) throw new Error("request body is too large");
    chunks.push(chunk);
  }}
  return JSON.parse(Buffer.concat(chunks).toString("utf8") || "{{}}");
}}

async function api(request, response, pathname) {{
  const match = pathname.match(/^\\/api\\/([A-Za-z_][A-Za-z0-9_]*)(?:\\/([^/]+))?$/);
  if (!match) return false;
  const [, name, id] = match;
  const resource = resourceMap.get(name);
  if (!resource) return send(response, 404, {{ error: "resource not found" }}), true;
  if (request.method === "GET" && !id && resource.actions.includes("list")) {{
    const database = await store();
    database[name] ||= [];
    send(response, 200, {{ data: database[name] }});
    return true;
  }}
  if (request.method === "POST" && !id && resource.actions.includes("create")) {{
    try {{
      const now = new Date().toISOString();
      const item = {{ id: randomUUID(), ...validate(resource, await body(request)), created_at: now, updated_at: now }};
      await mutate(database => {{ database[name] ||= []; database[name].push(item); }});
      send(response, 201, {{ data: item }});
    }} catch (error) {{
      send(response, 400, {{ error: error.message }});
    }}
    return true;
  }}
  if (request.method === "PUT" && id && resource.actions.includes("update")) {{
    try {{
      const values = validate(resource, await body(request));
      const updated = await mutate(database => {{
        database[name] ||= [];
        const index = database[name].findIndex(item => item.id === id);
        if (index < 0) return null;
        database[name][index] = {{ ...database[name][index], ...values, updated_at: new Date().toISOString() }};
        return database[name][index];
      }});
      if (!updated) send(response, 404, {{ error: "item not found" }});
      else send(response, 200, {{ data: updated }});
    }} catch (error) {{
      send(response, 400, {{ error: error.message }});
    }}
    return true;
  }}
  if (request.method === "DELETE" && id && resource.actions.includes("delete")) {{
    const found = await mutate(database => {{
      database[name] ||= [];
      const before = database[name].length;
      database[name] = database[name].filter(item => item.id !== id);
      return database[name].length !== before;
    }});
    if (!found) send(response, 404, {{ error: "item not found" }});
    else send(response, 200, {{ data: {{ id }} }});
    return true;
  }}
  send(response, 405, {{ error: "method not allowed" }});
  return true;
}}

const mime = {{ ".css": "text/css", ".html": "text/html", ".js": "text/javascript", ".svg": "image/svg+xml" }};

async function staticFile(response, pathname) {{
  const requested = pathname === "/" ? "index.html" : pathname.slice(1);
  const resolved = path.resolve(publicDirectory, requested);
  if (!resolved.startsWith(publicDirectory + path.sep) && resolved !== path.join(publicDirectory, "index.html")) {{
    return send(response, 403, {{ error: "forbidden" }});
  }}
  try {{
    const content = await readFile(resolved);
    response.writeHead(200, {{ "content-type": mime[path.extname(resolved)] || "application/octet-stream" }});
    response.end(content);
  }} catch (error) {{
    if (error.code !== "ENOENT") throw error;
    const content = await readFile(path.join(publicDirectory, "index.html"));
    response.writeHead(200, {{ "content-type": "text/html" }});
    response.end(content);
  }}
}}

const server = createServer(async (request, response) => {{
  try {{
    const url = new URL(request.url, "http://localhost");
    if (url.pathname === "/api/health") return send(response, 200, {{ ok: true }});
    if (await api(request, response, url.pathname)) return;
    await staticFile(response, url.pathname);
  }} catch (error) {{
    console.error(error);
    send(response, 500, {{ error: "internal server error" }});
  }}
}});

const port = Number(process.env.PORT || 3000);
server.listen(port, "127.0.0.1", () => console.log(`AML server: http://127.0.0.1:${{port}}`));
'''


def _postgres_migration(spec: FullstackSpec) -> str:
    sql_types = {
        "Text": "TEXT",
        "Int": "INTEGER",
        "Float": "DOUBLE PRECISION",
        "Bool": "BOOLEAN",
        "Time": "TIMESTAMPTZ",
    }
    statements: list[str] = []
    for resource in spec.resources:
        model = spec.models[resource.model]
        columns = [
            '  "id" UUID PRIMARY KEY',
            *[
                f'  "{name}" {sql_types[type_name]} NOT NULL'
                for name, type_name in model.fields
            ],
            '  "created_at" TIMESTAMPTZ NOT NULL DEFAULT NOW()',
            '  "updated_at" TIMESTAMPTZ NOT NULL DEFAULT NOW()',
        ]
        statements.append(
            f'CREATE TABLE IF NOT EXISTS "{resource.name}" (\n'
            + ",\n".join(columns)
            + "\n);"
        )
        statements.append(
            f'CREATE INDEX IF NOT EXISTS "idx_{resource.name}_created_at" '
            f'ON "{resource.name}" ("created_at" DESC);'
        )
    return "\n\n".join(statements) + "\n"


def _postgres_migrate_script() -> str:
    return '''import { readdir, readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import pg from "pg";

const { Pool } = pg;
if (!process.env.DATABASE_URL) throw new Error("DATABASE_URL is required");
const directory = path.dirname(fileURLToPath(import.meta.url));
const migrations = path.resolve(directory, "..", "migrations");
const pool = new Pool({ connectionString: process.env.DATABASE_URL, max: 2, connectionTimeoutMillis: 5000 });
const client = await pool.connect();

try {
  await client.query(`CREATE TABLE IF NOT EXISTS "_aml_migrations" (
    "name" TEXT PRIMARY KEY,
    "applied_at" TIMESTAMPTZ NOT NULL DEFAULT NOW()
  )`);
  const applied = new Set((await client.query('SELECT "name" FROM "_aml_migrations"')).rows.map(row => row.name));
  const files = (await readdir(migrations)).filter(name => name.endsWith(".sql")).sort();
  for (const name of files) {
    if (applied.has(name)) continue;
    const sql = await readFile(path.join(migrations, name), "utf8");
    await client.query("BEGIN");
    try {
      await client.query(sql);
      await client.query('INSERT INTO "_aml_migrations" ("name") VALUES ($1)', [name]);
      await client.query("COMMIT");
      console.log(`Applied ${name}`);
    } catch (error) {
      await client.query("ROLLBACK");
      throw error;
    }
  }
} finally {
  client.release();
  await pool.end();
}
'''


def _postgres_server(config_json: str) -> str:
    return f'''import {{ randomUUID }} from "node:crypto";
import {{ createServer }} from "node:http";
import {{ readFile }} from "node:fs/promises";
import path from "node:path";
import {{ fileURLToPath }} from "node:url";
import pg from "pg";

const {{ Pool }} = pg;
if (!process.env.DATABASE_URL) throw new Error("DATABASE_URL is required");
const directory = path.dirname(fileURLToPath(import.meta.url));
const publicDirectory = path.join(directory, "dist");
const resources = {config_json};
const resourceMap = new Map(resources.map(resource => [resource.name, resource]));
const pool = new Pool({{
  connectionString: process.env.DATABASE_URL,
  max: Number(process.env.DB_POOL_SIZE || 10),
  connectionTimeoutMillis: 5000,
  idleTimeoutMillis: 30000,
}});
pool.on("error", error => console.error("PostgreSQL pool error", error));
class AMLInputError extends Error {{}}

function identifier(name) {{
  if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(name)) throw new Error("unsafe SQL identifier");
  return `"${{name}}"`;
}}

function send(response, status, value) {{
  response.writeHead(status, {{ "content-type": "application/json; charset=utf-8" }});
  response.end(JSON.stringify(value));
}}

function valid(value, type) {{
  if (type === "Text") return typeof value === "string";
  if (type === "Int") return Number.isInteger(value);
  if (type === "Float") return typeof value === "number" && Number.isFinite(value);
  if (type === "Bool") return typeof value === "boolean";
  if (type === "Time") return typeof value === "string" && Number.isFinite(Date.parse(value));
  return false;
}}

function validate(resource, payload) {{
  const allowed = new Set(resource.fields.map(field => field.name));
  for (const key of Object.keys(payload)) if (!allowed.has(key)) throw new AMLInputError(`unknown field: ${{key}}`);
  const result = {{}};
  for (const field of resource.fields) {{
    const value = payload[field.name];
    if (!valid(value, field.type)) throw new AMLInputError(`${{field.name}} must be ${{field.type}}`);
    result[field.name] = value;
  }}
  return result;
}}

async function body(request) {{
  const chunks = [];
  let size = 0;
  for await (const chunk of request) {{
    size += chunk.length;
    if (size > 1_000_000) throw new AMLInputError("request body is too large");
    chunks.push(chunk);
  }}
  try {{
    return JSON.parse(Buffer.concat(chunks).toString("utf8") || "{{}}");
  }} catch {{
    throw new AMLInputError("request body must be valid JSON");
  }}
}}

async function list(resource) {{
  const result = await pool.query(`SELECT * FROM ${{identifier(resource.name)}} ORDER BY "created_at" DESC`);
  return result.rows;
}}

async function create(resource, payload) {{
  const values = validate(resource, payload);
  const fields = resource.fields.map(field => field.name);
  const columns = ["id", ...fields].map(identifier).join(", ");
  const parameters = [randomUUID(), ...fields.map(field => values[field])];
  const placeholders = parameters.map((_, index) => `$${{index + 1}}`).join(", ");
  const result = await pool.query(
    `INSERT INTO ${{identifier(resource.name)}} (${{columns}}) VALUES (${{placeholders}}) RETURNING *`,
    parameters,
  );
  return result.rows[0];
}}

async function update(resource, id, payload) {{
  const values = validate(resource, payload);
  const fields = resource.fields.map(field => field.name);
  const assignments = fields.map((field, index) => `${{identifier(field)}} = $${{index + 1}}`);
  assignments.push('"updated_at" = NOW()');
  const parameters = [...fields.map(field => values[field]), id];
  const result = await pool.query(
    `UPDATE ${{identifier(resource.name)}} SET ${{assignments.join(", ")}} WHERE "id" = $${{parameters.length}} RETURNING *`,
    parameters,
  );
  return result.rows[0] || null;
}}

async function remove(resource, id) {{
  const result = await pool.query(
    `DELETE FROM ${{identifier(resource.name)}} WHERE "id" = $1 RETURNING "id"`,
    [id],
  );
  return result.rows[0] || null;
}}

async function api(request, response, pathname) {{
  const match = pathname.match(/^\\/api\\/([A-Za-z_][A-Za-z0-9_]*)(?:\\/([^/]+))?$/);
  if (!match) return false;
  const [, name, id] = match;
  const resource = resourceMap.get(name);
  if (!resource) return send(response, 404, {{ error: "resource not found" }}), true;
  try {{
    if (request.method === "GET" && !id && resource.actions.includes("list")) {{
      send(response, 200, {{ data: await list(resource) }}); return true;
    }}
    if (request.method === "POST" && !id && resource.actions.includes("create")) {{
      send(response, 201, {{ data: await create(resource, await body(request)) }}); return true;
    }}
    if (request.method === "PUT" && id && resource.actions.includes("update")) {{
      const item = await update(resource, id, await body(request));
      send(response, item ? 200 : 404, item ? {{ data: item }} : {{ error: "item not found" }}); return true;
    }}
    if (request.method === "DELETE" && id && resource.actions.includes("delete")) {{
      const item = await remove(resource, id);
      send(response, item ? 200 : 404, item ? {{ data: item }} : {{ error: "item not found" }}); return true;
    }}
  }} catch (error) {{
    if (error instanceof AMLInputError) {{ send(response, 400, {{ error: error.message }}); return true; }}
    throw error;
  }}
  send(response, 405, {{ error: "method not allowed" }});
  return true;
}}

const mime = {{ ".css": "text/css", ".html": "text/html", ".js": "text/javascript", ".svg": "image/svg+xml" }};
async function staticFile(response, pathname) {{
  const requested = pathname === "/" ? "index.html" : pathname.slice(1);
  const resolved = path.resolve(publicDirectory, requested);
  if (!resolved.startsWith(publicDirectory + path.sep) && resolved !== path.join(publicDirectory, "index.html")) return send(response, 403, {{ error: "forbidden" }});
  try {{
    const content = await readFile(resolved);
    response.writeHead(200, {{ "content-type": mime[path.extname(resolved)] || "application/octet-stream" }});
    response.end(content);
  }} catch (error) {{
    if (error.code !== "ENOENT") throw error;
    const content = await readFile(path.join(publicDirectory, "index.html"));
    response.writeHead(200, {{ "content-type": "text/html" }});
    response.end(content);
  }}
}}

const server = createServer(async (request, response) => {{
  try {{
    const url = new URL(request.url, "http://localhost");
    if (url.pathname === "/api/health") {{ await pool.query("SELECT 1"); return send(response, 200, {{ ok: true }}); }}
    if (await api(request, response, url.pathname)) return;
    await staticFile(response, url.pathname);
  }} catch (error) {{ console.error(error); send(response, 500, {{ error: "internal server error" }}); }}
}});

async function shutdown() {{
  server.close(async () => {{ await pool.end(); process.exit(0); }});
}}
process.on("SIGINT", shutdown);
process.on("SIGTERM", shutdown);
const port = Number(process.env.PORT || 3000);
server.listen(port, "127.0.0.1", () => console.log(`AML server: http://127.0.0.1:${{port}}`));
'''


def _app(config_json: str, title: str) -> str:
    return f'''import {{ useCallback, useEffect, useState }} from "react";

const resources = {config_json};

function initialForm(resource) {{
  return Object.fromEntries(resource.fields.map(field => [
    field.name,
    field.type === "Bool" ? false : (field.type === "Text" || field.type === "Time") ? "" : 0,
  ]));
}}

function Field({{ field, value, onChange }}) {{
  if (field.type === "Bool") {{
    return <label className="checkbox"><input type="checkbox" checked={{value}} onChange={{event => onChange(event.target.checked)}} /> {{field.label}}</label>;
  }}
  const numeric = field.type === "Int" || field.type === "Float";
  const inputType = numeric ? "number" : field.type === "Time" ? "datetime-local" : "text";
  return (
    <label>
      <span>{{field.label}}</span>
      <input
        name={{field.name}}
        type={{inputType}}
        step={{field.type === "Float" ? "any" : undefined}}
        value={{value}}
        onChange={{event => onChange(numeric ? Number(event.target.value) : event.target.value)}}
        required
      />
    </label>
  );
}}

function ResourcePanel({{ resource }}) {{
  const [items, setItems] = useState([]);
  const [form, setForm] = useState(() => initialForm(resource));
  const [error, setError] = useState("");
  const [loading, setLoading] = useState(true);
  const [editingId, setEditingId] = useState(null);

  const load = useCallback(async () => {{
    setLoading(true);
    try {{
      const response = await fetch(`/api/${{resource.name}}`);
      const result = await response.json();
      if (!response.ok) throw new Error(result.error || "Unable to load data");
      setItems(result.data);
      setError("");
    }} catch (problem) {{ setError(problem.message); }}
    finally {{ setLoading(false); }}
  }}, [resource.name]);

  useEffect(() => {{ if (resource.actions.includes("list")) load(); }}, [load, resource.actions]);

  async function submit(event) {{
    event.preventDefault();
    const endpoint = editingId ? `/api/${{resource.name}}/${{editingId}}` : `/api/${{resource.name}}`;
    const response = await fetch(endpoint, {{
      method: editingId ? "PUT" : "POST",
      headers: {{ "content-type": "application/json" }},
      body: JSON.stringify(form),
    }});
    const result = await response.json();
    if (!response.ok) return setError(result.error || "Unable to create item");
    setForm(initialForm(resource));
    setEditingId(null);
    await load();
  }}

  function edit(item) {{
    setForm(Object.fromEntries(resource.fields.map(field => [
      field.name,
      field.type === "Time" && item[field.name] ? String(item[field.name]).slice(0, 16) : item[field.name],
    ])));
    setEditingId(item.id);
    window.scrollTo({{ top: 0, behavior: "smooth" }});
  }}

  async function remove(id) {{
    const response = await fetch(`/api/${{resource.name}}/${{id}}`, {{ method: "DELETE" }});
    const result = await response.json();
    if (!response.ok) return setError(result.error || "Unable to delete item");
    await load();
  }}

  return (
    <section className="panel">
      <div className="panel-heading"><div><p className="eyebrow">Resource</p><h2>{{resource.label}}</h2></div><button onClick={{load}}>Refresh</button></div>
      {{error && <p className="error" role="alert">{{error}}</p>}}
      {{resource.actions.includes("create") && (
        <form onSubmit={{submit}}>
          <div className="fields">{{resource.fields.map(field => <Field key={{field.name}} field={{field}} value={{form[field.name]}} onChange={{value => setForm(current => ({{ ...current, [field.name]: value }}))}} />)}}</div>
          <div className="form-actions"><button type="submit">{{editingId ? "Save changes" : `Add ${{resource.label}}`}}</button>{{editingId && <button type="button" className="secondary" onClick={{() => {{ setEditingId(null); setForm(initialForm(resource)); }}}}>Cancel</button>}}</div>
        </form>
      )}}
      {{loading ? <p>Loading…</p> : items.length === 0 ? <p className="empty">No records yet.</p> : (
        <div className="table-wrap"><table><thead><tr>{{resource.fields.map(field => <th key={{field.name}}>{{field.label}}</th>)}}<th>Actions</th></tr></thead><tbody>
          {{items.map(item => <tr key={{item.id}}>{{resource.fields.map(field => <td key={{field.name}}>{{String(item[field.name])}}</td>)}}<td><div className="row-actions">{{resource.actions.includes("update") && <button className="secondary" onClick={{() => edit(item)}}>Edit</button>}}{{resource.actions.includes("delete") && <button className="danger" onClick={{() => remove(item.id)}}>Delete</button>}}</div></td></tr>)}}
        </tbody></table></div>
      )}}
    </section>
  );
}}

export default function App() {{
  return <main><header><p className="eyebrow">AML full stack</p><h1>{title}</h1><p>Generated UI, API, validation, and persistent storage.</p></header>{{resources.map(resource => <ResourcePanel key={{resource.name}} resource={{resource}} />)}}</main>;
}}
'''


def fullstack_project_files(source: str) -> dict[str, str]:
    spec = parse_fullstack(source)
    config = _resource_config(spec)
    config_json = json.dumps(config, separators=(",", ":"))
    title = spec.page.replace("_", " ").title()
    package = {
        "name": spec.app.lower().replace("_", "-"),
        "private": True,
        "version": "0.0.0",
        "type": "module",
        "scripts": {
            "dev": "node scripts/dev.mjs",
            "build": "vite build",
            "start": "node server.mjs",
        },
        "dependencies": {
            "react": f"^{REACT_VERSION}",
            "react-dom": f"^{REACT_VERSION}",
        },
        "devDependencies": {
            "@vitejs/plugin-react": f"^{VITE_REACT_VERSION}",
            "vite": f"^{VITE_VERSION}",
        },
    }
    if spec.storage == "postgres":
        package["scripts"]["migrate"] = "node scripts/migrate.mjs"
        package["dependencies"]["pg"] = f"^{PG_VERSION}"
    vite = '''import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [react()],
  server: { proxy: { "/api": "http://127.0.0.1:3001" } },
});
'''
    dev = '''import { spawn } from "node:child_process";

const processes = [
  spawn(process.execPath, ["--watch", "server.mjs"], { stdio: "inherit", env: { ...process.env, PORT: "3001" } }),
  spawn(process.platform === "win32" ? "npx.cmd" : "npx", ["vite"], { stdio: "inherit" }),
];

function stop(signal) {
  for (const process of processes) process.kill(signal);
}

process.on("SIGINT", () => { stop("SIGINT"); process.exit(0); });
process.on("SIGTERM", () => { stop("SIGTERM"); process.exit(0); });
for (const child of processes) child.on("exit", code => { if (code) { stop("SIGTERM"); process.exit(code); } });
'''
    main = '''import { StrictMode } from "react";
import { createRoot } from "react-dom/client";
import App from "./App.jsx";
import "./styles.css";

createRoot(document.getElementById("root")).render(<StrictMode><App /></StrictMode>);
'''
    styles = '''
:root { font-family: Inter, ui-sans-serif, system-ui, sans-serif; color: #172033; background: #f2f5fa; }
* { box-sizing: border-box; }
body { margin: 0; min-width: 320px; min-height: 100vh; }
button, input { font: inherit; }
button { border: 0; border-radius: .55rem; padding: .65rem .9rem; color: white; background: #3157d5; cursor: pointer; }
button:hover { filter: brightness(.92); }
button:focus-visible, input:focus-visible { outline: 3px solid #8ba4ff; outline-offset: 2px; }
main { width: min(100% - 2rem, 76rem); margin: 0 auto; padding: 3rem 0; }
header { margin-bottom: 2rem; }
h1, h2, p { margin-top: 0; }
.eyebrow { margin-bottom: .35rem; color: #52627d; font-size: .75rem; font-weight: 800; letter-spacing: .12em; text-transform: uppercase; }
.panel { margin-bottom: 1.5rem; padding: 1.5rem; border: 1px solid #dce3ef; border-radius: 1rem; background: white; box-shadow: 0 1rem 3rem rgb(28 43 75 / 8%); }
.panel-heading { display: flex; align-items: center; justify-content: space-between; gap: 1rem; }
.fields { display: grid; grid-template-columns: repeat(auto-fit, minmax(12rem, 1fr)); gap: 1rem; margin: 1rem 0; }
label span { display: block; margin-bottom: .35rem; font-weight: 700; }
input { width: 100%; border: 1px solid #bdc7da; border-radius: .55rem; padding: .65rem .75rem; background: white; }
.checkbox { display: flex; align-items: center; gap: .5rem; }
.checkbox input { width: auto; }
.table-wrap { overflow-x: auto; margin-top: 1.25rem; }
table { width: 100%; border-collapse: collapse; }
th, td { padding: .75rem; border-bottom: 1px solid #e5eaf2; text-align: left; }
th { color: #52627d; font-size: .8rem; text-transform: uppercase; }
.danger { background: #a62d3d; }
.secondary { color: #24324b; background: #e7ecf5; }
.form-actions, .row-actions { display: flex; flex-wrap: wrap; gap: .5rem; }
.error { padding: .75rem; border-radius: .5rem; color: #8a1c1c; background: #fff0f0; }
.empty { margin: 1.25rem 0 0; color: #65748c; }
'''.lstrip()
    index = f'''<!doctype html><html lang="en"><head><meta charset="UTF-8" /><meta name="viewport" content="width=device-width, initial-scale=1.0" /><meta name="description" content="{title}, generated by AML" /><title>{title}</title></head><body><div id="root"></div><script type="module" src="/src/main.jsx"></script></body></html>\n'''
    migrate_command = "npm run migrate\n" if spec.storage == "postgres" else ""
    if spec.storage == "postgres":
        storage_setup = '''Set `DATABASE_URL`, then apply generated migrations before starting the app:

```sh
export DATABASE_URL="postgres://user:password@localhost:5432/database"
npm run migrate
```
'''
        environment_help = "Set `DATABASE_URL` for PostgreSQL, `DB_POOL_SIZE` to tune the pool, and `PORT` to change the server port."
    else:
        storage_setup = "The default JSON store works without an external database.\n"
        environment_help = "Set `AML_DATA_FILE` to move the persistent JSON store and `PORT` to change the server port."
    readme = f'''# {title}

```sh
npm install
{migrate_command}npm run dev
```

{storage_setup}

Production:

```sh
npm run build
npm start
```

{environment_help}
'''
    initial_store = {resource.name: [] for resource in spec.resources}
    files = {
        ".gitignore": "node_modules/\ndist/\n.env\n.env.*\ndata/*.tmp\n",
        "README.md": readme,
        "index.html": index,
        "package.json": json.dumps(package, indent=2) + "\n",
        "server.mjs": (
            _postgres_server(config_json) if spec.storage == "postgres" else _server(config_json)
        ),
        "vite.config.js": vite,
        "scripts/dev.mjs": dev,
        "src/App.jsx": _app(config_json, title),
        "src/main.jsx": main,
        "src/styles.css": styles,
    }
    if spec.storage == "postgres":
        files[".env.example"] = "DATABASE_URL=postgres://user:password@localhost:5432/database\nDB_POOL_SIZE=10\nPORT=3000\n"
        files["migrations/001_initial.sql"] = _postgres_migration(spec)
        files["scripts/migrate.mjs"] = _postgres_migrate_script()
    else:
        files["data/store.json"] = json.dumps(initial_store, indent=2) + "\n"
    return files

