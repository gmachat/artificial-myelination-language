use std::collections::{BTreeMap, BTreeSet};

use crate::{web, CompileError};

const PG_VERSION: &str = "8.23.1";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Model {
    pub name: String,
    pub fields: Vec<(String, String)>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Resource {
    pub name: String,
    pub model: String,
    pub actions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Spec {
    pub app: String,
    pub page: String,
    pub path: String,
    pub storage: String,
    pub models: BTreeMap<String, Model>,
    pub resources: Vec<Resource>,
}

#[derive(Clone)]
struct Line {
    number: usize,
    indent: usize,
    text: String,
}

fn valid_name(value: &str) -> bool {
    let mut chars = value.chars();
    matches!(chars.next(), Some(first) if first.is_ascii_alphabetic() || first == '_')
        && chars.all(|item| item.is_ascii_alphanumeric() || item == '_')
}

fn lines(source: &str) -> Result<Vec<Line>, CompileError> {
    let mut output = Vec::new();
    for (offset, raw) in source.lines().enumerate() {
        let number = offset + 1;
        if raw.contains('\t') {
            return Err(CompileError::new(
                "tabs are not allowed; use two spaces",
                number,
            ));
        }
        if raw.trim().is_empty() || raw.trim_start().starts_with('#') {
            continue;
        }
        let spaces = raw.len() - raw.trim_start_matches(' ').len();
        if spaces % 2 != 0 {
            return Err(CompileError::new(
                "indentation must use multiples of two spaces",
                number,
            ));
        }
        output.push(Line {
            number,
            indent: spaces / 2,
            text: raw.trim().into(),
        });
    }
    Ok(output)
}

fn declaration<'a>(text: &'a str, prefix: &str, suffix: &str) -> Option<&'a str> {
    text.strip_prefix(prefix)?.strip_suffix(suffix)
}

pub fn parse(source: &str) -> Result<Spec, CompileError> {
    let lines = lines(source)?;
    if lines.len() < 2 {
        return Err(CompileError::new(
            "full-stack source requires app and profile declarations",
            1,
        ));
    }
    let app = lines[0]
        .text
        .strip_prefix("app ")
        .filter(|name| valid_name(name));
    if lines[0].indent != 0 || app.is_none() {
        return Err(CompileError::new(
            "the first declaration must be 'app <name>'",
            lines[0].number,
        ));
    }
    if lines[1].indent != 0 || lines[1].text != "profile fullstack" {
        return Err(CompileError::new(
            "full-stack target requires 'profile fullstack'",
            lines[1].number,
        ));
    }
    let mut models = BTreeMap::new();
    let mut resources = Vec::new();
    let mut storage = "json".to_string();
    let mut storage_seen = false;
    let mut page: Option<(String, String)> = None;
    let mut page_resources: Vec<String> = Vec::new();
    let mut index = 2;
    while index < lines.len() {
        let line = &lines[index];
        if line.indent != 0 {
            return Err(CompileError::new(
                "unexpected top-level indentation",
                line.number,
            ));
        }
        if let Some(value) = line.text.strip_prefix("storage ") {
            if storage_seen || !matches!(value, "json" | "postgres") {
                return Err(CompileError::new(
                    "storage must be json or postgres and declared once",
                    line.number,
                ));
            }
            storage = value.into();
            storage_seen = true;
            index += 1;
            continue;
        }
        if let Some(name) = declaration(&line.text, "data ", ":") {
            if !valid_name(name) || !name.chars().next().is_some_and(char::is_uppercase) {
                return Err(CompileError::new(
                    "data names must begin with an uppercase letter",
                    line.number,
                ));
            }
            if models.contains_key(name) {
                return Err(CompileError::new(
                    format!("duplicate data declaration '{name}'"),
                    line.number,
                ));
            }
            let model_line = line.number;
            let mut fields = Vec::new();
            index += 1;
            while index < lines.len() && lines[index].indent > 0 {
                let field = &lines[index];
                if field.indent != 1 {
                    return Err(CompileError::new(
                        "data fields use one indentation level",
                        field.number,
                    ));
                }
                let Some((field_name, kind)) = field.text.split_once(':') else {
                    return Err(CompileError::new(
                        "data fields use 'name: Type'",
                        field.number,
                    ));
                };
                let field_name = field_name.trim();
                let kind = kind.trim();
                if !valid_name(field_name) || !valid_name(kind) {
                    return Err(CompileError::new(
                        "data fields use 'name: Type'",
                        field.number,
                    ));
                }
                if field_name == "id" {
                    return Err(CompileError::new(
                        "id is generated automatically",
                        field.number,
                    ));
                }
                if fields.iter().any(|(existing, _)| existing == field_name) {
                    return Err(CompileError::new(
                        format!("duplicate field '{field_name}'"),
                        field.number,
                    ));
                }
                if !matches!(kind, "Text" | "Int" | "Float" | "Bool" | "Time") {
                    return Err(CompileError::new(
                        "persistent field type must be one of: Bool, Float, Int, Text, Time",
                        field.number,
                    ));
                }
                fields.push((field_name.into(), kind.into()));
                index += 1;
            }
            if fields.is_empty() {
                return Err(CompileError::new(
                    "data declaration requires at least one field",
                    model_line,
                ));
            }
            models.insert(
                name.into(),
                Model {
                    name: name.into(),
                    fields,
                },
            );
            continue;
        }
        if let Some(raw) = declaration(&line.text, "resource ", ":") {
            let Some((name, model)) = raw.split_once(':') else {
                return Err(CompileError::new(
                    "resource uses 'resource name: Model:'",
                    line.number,
                ));
            };
            let (name, model) = (name.trim(), model.trim());
            if !valid_name(name) || !valid_name(model) {
                return Err(CompileError::new(
                    "resource uses 'resource name: Model:'",
                    line.number,
                ));
            }
            if resources.iter().any(|item: &Resource| item.name == name) {
                return Err(CompileError::new(
                    format!("duplicate resource '{name}'"),
                    line.number,
                ));
            }
            let resource_line = line.number;
            let mut actions = Vec::new();
            index += 1;
            while index < lines.len() && lines[index].indent > 0 {
                let action = &lines[index];
                if action.indent != 1
                    || !matches!(
                        action.text.as_str(),
                        "list" | "create" | "update" | "delete"
                    )
                {
                    return Err(CompileError::new(
                        "resource action must be list, create, update, or delete",
                        action.number,
                    ));
                }
                if actions.contains(&action.text) {
                    return Err(CompileError::new(
                        format!("duplicate resource action '{}'", action.text),
                        action.number,
                    ));
                }
                actions.push(action.text.clone());
                index += 1;
            }
            if actions.is_empty() {
                return Err(CompileError::new(
                    "resource requires at least one action",
                    resource_line,
                ));
            }
            resources.push(Resource {
                name: name.into(),
                model: model.into(),
                actions,
            });
            continue;
        }
        if let Some(raw) = declaration(&line.text, "page ", ":") {
            let Some((name, route)) = raw.split_once(" at ") else {
                return Err(CompileError::new(
                    "page uses 'page Name at \"/path\":'",
                    line.number,
                ));
            };
            let route = route
                .strip_prefix('"')
                .and_then(|item| item.strip_suffix('"'));
            if page.is_some() || !valid_name(name) || route.is_none() {
                return Err(CompileError::new(
                    "fullstack supports one valid page",
                    line.number,
                ));
            }
            page = Some((name.into(), route.unwrap().into()));
            index += 1;
            while index < lines.len() && lines[index].indent > 0 {
                let item = &lines[index];
                let resource = item
                    .text
                    .strip_prefix("resource ")
                    .filter(|name| valid_name(name));
                if item.indent != 1 || resource.is_none() {
                    return Err(CompileError::new(
                        "page entries use 'resource <name>'",
                        item.number,
                    ));
                }
                page_resources.push(resource.unwrap().into());
                index += 1;
            }
            continue;
        }
        return Err(CompileError::new(
            format!("unknown full-stack declaration: {}", line.text),
            line.number,
        ));
    }
    for resource in &resources {
        if !models.contains_key(&resource.model) {
            return Err(CompileError::new(
                format!(
                    "resource '{}' references unknown data '{}'",
                    resource.name, resource.model
                ),
                1,
            ));
        }
    }
    let known: BTreeSet<_> = resources.iter().map(|item| item.name.as_str()).collect();
    for name in &page_resources {
        if !known.contains(name.as_str()) {
            return Err(CompileError::new(
                format!("page references unknown resource '{name}'"),
                1,
            ));
        }
    }
    let Some((page, path)) = page else {
        return Err(CompileError::new(
            "full-stack application requires a page",
            lines.last().map_or(1, |line| line.number),
        ));
    };
    let resources: Vec<_> = resources
        .into_iter()
        .filter(|item| page_resources.contains(&item.name))
        .collect();
    if resources.is_empty() {
        return Err(CompileError::new("page requires at least one resource", 1));
    }
    Ok(Spec {
        app: app.unwrap().into(),
        page,
        path,
        storage,
        models,
        resources,
    })
}

fn json(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn title(value: &str) -> String {
    value
        .split('_')
        .map(|part| {
            let mut chars = part.chars();
            chars.next().map_or(String::new(), |first| {
                first.to_uppercase().chain(chars).collect()
            })
        })
        .collect::<Vec<_>>()
        .join(" ")
}

fn config(spec: &Spec) -> String {
    let resources = spec
        .resources
        .iter()
        .map(|resource| {
            let model = &spec.models[&resource.model];
            let fields = model
                .fields
                .iter()
                .map(|(name, kind)| {
                    format!(
                        "{{\"name\":{},\"label\":{},\"type\":{}}}",
                        json(name),
                        json(&title(name)),
                        json(kind)
                    )
                })
                .collect::<Vec<_>>()
                .join(",");
            let actions = resource
                .actions
                .iter()
                .map(|value| json(value))
                .collect::<Vec<_>>()
                .join(",");
            format!(
                "{{\"name\":{},\"label\":{},\"actions\":[{actions}],\"fields\":[{fields}]}}",
                json(&resource.name),
                json(&title(&resource.name))
            )
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("[{resources}]")
}

const JSON_SERVER: &str = r#"import { randomUUID } from "node:crypto";
import { createServer } from "node:http";
import { readFile, rename, writeFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
const directory = path.dirname(fileURLToPath(import.meta.url));
const dataFile = process.env.AML_DATA_FILE || path.join(directory, "data", "store.json");
const publicDirectory = path.join(directory, "dist");
const resources = __CONFIG__;
const resourceMap = new Map(resources.map(resource => [resource.name, resource]));
let mutationQueue = Promise.resolve();
function send(response, status, value) { response.writeHead(status, { "content-type": "application/json; charset=utf-8" }); response.end(JSON.stringify(value)); }
async function store() { try { return JSON.parse(await readFile(dataFile, "utf8")); } catch (error) { if (error.code === "ENOENT") return Object.fromEntries(resources.map(item => [item.name, []])); throw error; } }
function mutate(operation) { const next = mutationQueue.then(async () => { const value = await store(); const result = await operation(value); await writeFile(`${dataFile}.tmp`, JSON.stringify(value, null, 2) + "\n"); await rename(`${dataFile}.tmp`, dataFile); return result; }); mutationQueue = next.then(() => undefined, () => undefined); return next; }
function valid(value, type) { if (type === "Text") return typeof value === "string" && value.trim().length > 0; if (type === "Int") return Number.isInteger(value); if (type === "Float") return typeof value === "number" && Number.isFinite(value); if (type === "Bool") return typeof value === "boolean"; if (type === "Time") return typeof value === "string" && Number.isFinite(Date.parse(value)); return false; }
function validate(resource, payload) { const allowed = new Set(resource.fields.map(field => field.name)); for (const key of Object.keys(payload)) if (!allowed.has(key)) throw new Error(`unknown field: ${key}`); const result = {}; for (const field of resource.fields) { if (!valid(payload[field.name], field.type)) throw new Error(`${field.name} must be ${field.type}`); result[field.name] = payload[field.name]; } return result; }
async function body(request) { const chunks = []; let size = 0; for await (const chunk of request) { size += chunk.length; if (size > 1_000_000) throw new Error("request body is too large"); chunks.push(chunk); } return JSON.parse(Buffer.concat(chunks).toString("utf8") || "{}"); }
async function api(request, response, pathname) { const match = pathname.match(/^\/api\/([A-Za-z_][A-Za-z0-9_]*)(?:\/([^/]+))?$/); if (!match) return false; const [, name, id] = match; const resource = resourceMap.get(name); if (!resource) { send(response, 404, { error: "resource not found" }); return true; }
  if (request.method === "GET" && !id && resource.actions.includes("list")) { const data = await store(); send(response, 200, { data: data[name] || [] }); return true; }
  if (request.method === "POST" && !id && resource.actions.includes("create")) { try { const now = new Date().toISOString(); const item = { id: randomUUID(), ...validate(resource, await body(request)), created_at: now, updated_at: now }; await mutate(data => { data[name] ||= []; data[name].push(item); }); send(response, 201, { data: item }); } catch (error) { send(response, 400, { error: error.message }); } return true; }
  if (request.method === "PUT" && id && resource.actions.includes("update")) { try { const values = validate(resource, await body(request)); const item = await mutate(data => { data[name] ||= []; const index = data[name].findIndex(value => value.id === id); if (index < 0) return null; data[name][index] = { ...data[name][index], ...values, updated_at: new Date().toISOString() }; return data[name][index]; }); send(response, item ? 200 : 404, item ? { data: item } : { error: "item not found" }); } catch (error) { send(response, 400, { error: error.message }); } return true; }
  if (request.method === "DELETE" && id && resource.actions.includes("delete")) { const found = await mutate(data => { data[name] ||= []; const before = data[name].length; data[name] = data[name].filter(value => value.id !== id); return before !== data[name].length; }); send(response, found ? 200 : 404, found ? { data: { id } } : { error: "item not found" }); return true; }
  send(response, 405, { error: "method not allowed" }); return true; }
async function staticFile(response, pathname) { const requested = pathname === "/" ? "index.html" : pathname.slice(1); const resolved = path.resolve(publicDirectory, requested); if (!resolved.startsWith(publicDirectory + path.sep) && resolved !== path.join(publicDirectory, "index.html")) return send(response, 403, { error: "forbidden" }); try { response.end(await readFile(resolved)); } catch (error) { if (error.code !== "ENOENT") throw error; response.end(await readFile(path.join(publicDirectory, "index.html"))); } }
const server = createServer(async (request, response) => { try { const url = new URL(request.url, "http://localhost"); if (url.pathname === "/api/health") return send(response, 200, { ok: true }); if (await api(request, response, url.pathname)) return; await staticFile(response, url.pathname); } catch (error) { console.error(error); send(response, 500, { error: "internal server error" }); } });
const port = Number(process.env.PORT || 3000); server.listen(port, "127.0.0.1", () => console.log(`AML server: http://127.0.0.1:${port}`));
"#;

const POSTGRES_SERVER: &str = r#"import { randomUUID } from "node:crypto";
import { createServer } from "node:http";
import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import pg from "pg";
const { Pool } = pg;
if (!process.env.DATABASE_URL) throw new Error("DATABASE_URL is required");
const resources = __CONFIG__; const resourceMap = new Map(resources.map(resource => [resource.name, resource]));
const directory = path.dirname(fileURLToPath(import.meta.url)); const publicDirectory = path.join(directory, "dist");
const pool = new Pool({ connectionString: process.env.DATABASE_URL, max: Number(process.env.DB_POOL_SIZE || 10) });
function identifier(name) { if (!/^[A-Za-z_][A-Za-z0-9_]*$/.test(name)) throw new Error("unsafe SQL identifier"); return `"${name}"`; }
function send(response, status, value) { response.writeHead(status, { "content-type": "application/json; charset=utf-8" }); response.end(JSON.stringify(value)); }
function valid(value, type) { if (type === "Text") return typeof value === "string" && value.trim().length > 0; if (type === "Int") return Number.isInteger(value); if (type === "Float") return typeof value === "number" && Number.isFinite(value); if (type === "Bool") return typeof value === "boolean"; if (type === "Time") return typeof value === "string" && Number.isFinite(Date.parse(value)); return false; }
function validate(resource, payload) { const allowed = new Set(resource.fields.map(field => field.name)); for (const key of Object.keys(payload)) if (!allowed.has(key)) throw new Error(`unknown field: ${key}`); const values = {}; for (const field of resource.fields) { if (!valid(payload[field.name], field.type)) throw new Error(`${field.name} must be ${field.type}`); values[field.name] = payload[field.name]; } return values; }
async function body(request) { const chunks = []; let size = 0; for await (const chunk of request) { size += chunk.length; if (size > 1_000_000) throw new Error("request body is too large"); chunks.push(chunk); } return JSON.parse(Buffer.concat(chunks).toString("utf8") || "{}"); }
async function list(resource) { return (await pool.query(`SELECT * FROM ${identifier(resource.name)} ORDER BY "created_at" DESC`)).rows; }
async function create(resource, payload) { const values = validate(resource, payload); const fields = resource.fields.map(field => field.name); const parameters = [randomUUID(), ...fields.map(field => values[field])]; const columns = ["id", ...fields].map(identifier).join(", "); const places = parameters.map((_, index) => `$${index + 1}`).join(", "); return (await pool.query(`INSERT INTO ${identifier(resource.name)} (${columns}) VALUES (${places}) RETURNING *`, parameters)).rows[0]; }
async function update(resource, id, values) { const fields = resource.fields.map(field => field.name); const assignments = fields.map((field, index) => `${identifier(field)} = $${index + 1}`); const parameters = [...fields.map(field => values[field]), id]; return (await pool.query(`UPDATE ${identifier(resource.name)} SET ${assignments.join(", ")}, "updated_at" = NOW() WHERE "id" = $${parameters.length} RETURNING *`, parameters)).rows[0] || null; }
async function remove(resource, id) { return (await pool.query(`DELETE FROM ${identifier(resource.name)} WHERE "id" = $1 RETURNING "id"`, [id])).rows[0] || null; }
async function api(request, response, pathname) { const match = pathname.match(/^\/api\/([A-Za-z_][A-Za-z0-9_]*)(?:\/([^/]+))?$/); if (!match) return false; const [, name, id] = match; const resource = resourceMap.get(name); if (!resource) { send(response, 404, { error: "resource not found" }); return true; } try {
  if (request.method === "GET" && !id && resource.actions.includes("list")) { send(response, 200, { data: await list(resource) }); return true; }
  if (request.method === "POST" && !id && resource.actions.includes("create")) { send(response, 201, { data: await create(resource, await body(request)) }); return true; }
  if (request.method === "PUT" && id && resource.actions.includes("update")) { const item = await update(resource, id, validate(resource, await body(request))); send(response, item ? 200 : 404, item ? { data: item } : { error: "item not found" }); return true; }
  if (request.method === "DELETE" && id && resource.actions.includes("delete")) { const item = await remove(resource, id); send(response, item ? 200 : 404, item ? { data: item } : { error: "item not found" }); return true; }
  send(response, 405, { error: "method not allowed" }); return true;
} catch (error) { send(response, 400, { error: error.message }); return true; } }
async function staticFile(response, pathname) { const requested = pathname === "/" ? "index.html" : pathname.slice(1); const resolved = path.resolve(publicDirectory, requested); if (!resolved.startsWith(publicDirectory + path.sep) && resolved !== path.join(publicDirectory, "index.html")) return send(response, 403, { error: "forbidden" }); try { response.end(await readFile(resolved)); } catch (error) { if (error.code !== "ENOENT") throw error; response.end(await readFile(path.join(publicDirectory, "index.html"))); } }
const server = createServer(async (request, response) => { try { const url = new URL(request.url, "http://localhost"); if (url.pathname === "/api/health") { await pool.query("SELECT 1"); return send(response, 200, { ok: true }); } if (await api(request, response, url.pathname)) return; await staticFile(response, url.pathname); } catch (error) { console.error(error); send(response, 500, { error: "internal server error" }); } });
async function shutdown() { server.close(async () => { await pool.end(); process.exit(0); }); }
process.on("SIGINT", shutdown); process.on("SIGTERM", shutdown);
server.listen(Number(process.env.PORT || 3000), "127.0.0.1");
"#;

fn app(config: &str, page: &str) -> String {
    format!(
        r#"import {{ useEffect, useState }} from "react";
const resources = {config};
function initial(resource) {{ return Object.fromEntries(resource.fields.map(field => [field.name, field.type === "Bool" ? false : field.type === "Text" || field.type === "Time" ? "" : 0])); }}
function Panel({{ resource }}) {{
  const [items, setItems] = useState([]); const [form, setForm] = useState(() => initial(resource)); const [editingId, setEditingId] = useState(null); const [error, setError] = useState("");
  async function load() {{ const response = await fetch(`/api/${{resource.name}}`); const value = await response.json(); if (response.ok) setItems(value.data); else setError(value.error); }}
  useEffect(() => {{ if (resource.actions.includes("list")) load(); }}, []);
  async function submit(event) {{ event.preventDefault(); const response = await fetch(`/api/${{resource.name}}${{editingId ? `/` + editingId : ""}}`, {{ method: editingId ? "PUT" : "POST", headers: {{ "content-type": "application/json" }}, body: JSON.stringify(form) }}); if (!response.ok) setError((await response.json()).error); else {{ setEditingId(null); setForm(initial(resource)); load(); }} }}
  async function remove(id) {{ await fetch(`/api/${{resource.name}}/${{id}}`, {{ method: "DELETE" }}); load(); }}
  return <section><h2>{{resource.label}}</h2>{{error && <p role="alert">{{error}}</p>}}<form onSubmit={{submit}}>{{resource.fields.map(field => <label key={{field.name}}>{{field.label}}<input value={{String(form[field.name])}} onChange={{event => setForm(current => ({{...current, [field.name]: field.type === "Bool" ? event.target.checked : field.type === "Int" || field.type === "Float" ? Number(event.target.value) : event.target.value}}))}} /></label>)}}<button>{{editingId ? "Save" : "Add"}}</button></form><table><tbody>{{items.map(item => <tr key={{item.id}}>{{resource.fields.map(field => <td key={{field.name}}>{{String(item[field.name])}}</td>)}}<td>{{resource.actions.includes("update") && <button onClick={{() => {{ setEditingId(item.id); setForm(item); }}}}>Edit</button>}}{{resource.actions.includes("delete") && <button onClick={{() => remove(item.id)}}>Delete</button>}}</td></tr>)}}</tbody></table></section>;
}}
export default function App() {{ return <main><h1>{}</h1>{{resources.map(resource => <Panel key={{resource.name}} resource={{resource}} />)}}</main>; }}
"#,
        title(page)
    )
}

fn migration(spec: &Spec) -> String {
    let mut output = Vec::new();
    for resource in &spec.resources {
        let model = &spec.models[&resource.model];
        let mut columns = vec!["  \"id\" UUID PRIMARY KEY".to_string()];
        for (name, kind) in &model.fields {
            let sql = match kind.as_str() {
                "Text" => "TEXT",
                "Int" => "INTEGER",
                "Float" => "DOUBLE PRECISION",
                "Bool" => "BOOLEAN",
                _ => "TIMESTAMPTZ",
            };
            columns.push(format!("  \"{name}\" {sql} NOT NULL"));
        }
        columns.push("  \"created_at\" TIMESTAMPTZ NOT NULL DEFAULT NOW()".into());
        columns.push("  \"updated_at\" TIMESTAMPTZ NOT NULL DEFAULT NOW()".into());
        output.push(format!(
            "CREATE TABLE IF NOT EXISTS \"{}\" (\n{}\n);",
            resource.name,
            columns.join(",\n")
        ));
    }
    output.join("\n\n") + "\n"
}

pub fn project_files(source: &str) -> Result<BTreeMap<String, String>, CompileError> {
    let spec = parse(source)?;
    let config = config(&spec);
    let package_name = spec.app.to_lowercase().replace('_', "-");
    let pg_dependency = if spec.storage == "postgres" {
        format!(",\n    \"pg\": \"^{PG_VERSION}\"")
    } else {
        String::new()
    };
    let migrate = if spec.storage == "postgres" {
        ",\n    \"migrate\": \"node scripts/migrate.mjs\""
    } else {
        ""
    };
    let package = format!(
        r#"{{
  "name": "{package_name}", "private": true, "version": "0.0.0", "type": "module",
  "scripts": {{ "dev": "node scripts/dev.mjs", "build": "vite build", "start": "node server.mjs"{migrate} }},
  "dependencies": {{ "react": "^{}", "react-dom": "^{}"{} }},
  "devDependencies": {{ "@vitejs/plugin-react": "^{}", "vite": "^{}" }}
}}
"#,
        web::REACT_VERSION,
        web::REACT_VERSION,
        pg_dependency,
        web::VITE_REACT_VERSION,
        web::VITE_VERSION
    );
    let mut files = BTreeMap::from([
        (".gitignore".into(), "node_modules/\ndist/\n.env\n.env.*\ndata/*.tmp\n".into()),
        ("README.md".into(), format!("# {}\n\n```sh\nnpm install\nnpm run dev\n```\n", title(&spec.page))),
        ("index.html".into(), format!("<!doctype html><html><head><meta charset=\"UTF-8\"><meta name=\"viewport\" content=\"width=device-width,initial-scale=1\"><title>{}</title></head><body><div id=\"root\"></div><script type=\"module\" src=\"/src/main.jsx\"></script></body></html>\n", title(&spec.page))),
        ("package.json".into(), package),
        ("server.mjs".into(), if spec.storage == "postgres" { POSTGRES_SERVER.replace("__CONFIG__", &config) } else { JSON_SERVER.replace("__CONFIG__", &config) }),
        ("vite.config.js".into(), "import { defineConfig } from \"vite\";\nimport react from \"@vitejs/plugin-react\";\nexport default defineConfig({ plugins: [react()], server: { proxy: { \"/api\": \"http://127.0.0.1:3001\" } } });\n".into()),
        ("scripts/dev.mjs".into(), "import { spawn } from \"node:child_process\";\nconst api = spawn(process.execPath, [\"--watch\", \"server.mjs\"], { stdio: \"inherit\", env: { ...process.env, PORT: \"3001\" } });\nconst web = spawn(process.platform === \"win32\" ? \"npx.cmd\" : \"npx\", [\"vite\"], { stdio: \"inherit\" });\nprocess.on(\"SIGINT\", () => { api.kill(); web.kill(); process.exit(0); });\n".into()),
        ("src/App.jsx".into(), app(&config, &spec.page)),
        ("src/main.jsx".into(), "import { StrictMode } from \"react\";\nimport { createRoot } from \"react-dom/client\";\nimport App from \"./App.jsx\";\nimport \"./styles.css\";\ncreateRoot(document.getElementById(\"root\")).render(<StrictMode><App /></StrictMode>);\n".into()),
        ("src/styles.css".into(), ":root{font-family:Inter,system-ui;color:#172033;background:#f2f5fa}*{box-sizing:border-box}body{margin:0}main{width:min(100% - 2rem,76rem);margin:auto;padding:3rem 0}section{background:white;padding:1.5rem;margin:1rem 0;border-radius:1rem}label{display:block;margin:.5rem 0}input,button{font:inherit;padding:.6rem;margin:.25rem}table{width:100%}\n".into()),
    ]);
    if spec.storage == "postgres" {
        files.insert(".env.example".into(), "DATABASE_URL=postgres://user:password@localhost:5432/database\nDB_POOL_SIZE=10\nPORT=3000\n".into());
        files.insert("migrations/001_initial.sql".into(), migration(&spec));
        files.insert("scripts/migrate.mjs".into(), r#"import { readFile } from "node:fs/promises";
import path from "node:path";
import { fileURLToPath } from "node:url";
import pg from "pg";
if (!process.env.DATABASE_URL) throw new Error("DATABASE_URL is required");
const pool = new pg.Pool({ connectionString: process.env.DATABASE_URL, max: 2 });
const client = await pool.connect();
try {
  await client.query('CREATE TABLE IF NOT EXISTS "_aml_migrations" ("name" TEXT PRIMARY KEY, "applied_at" TIMESTAMPTZ NOT NULL DEFAULT NOW())');
  const name = "001_initial.sql";
  const exists = await client.query('SELECT 1 FROM "_aml_migrations" WHERE "name" = $1', [name]);
  if (!exists.rowCount) {
    const directory = path.dirname(fileURLToPath(import.meta.url));
    const sql = await readFile(path.resolve(directory, "..", "migrations", name), "utf8");
    await client.query("BEGIN");
    try { await client.query(sql); await client.query('INSERT INTO "_aml_migrations" ("name") VALUES ($1)', [name]); await client.query("COMMIT"); }
    catch (error) { await client.query("ROLLBACK"); throw error; }
  }
} finally { client.release(); await pool.end(); }
"#.into());
    } else {
        let entries = spec
            .resources
            .iter()
            .map(|resource| format!("  {}: []", json(&resource.name)))
            .collect::<Vec<_>>()
            .join(",\n");
        files.insert("data/store.json".into(), format!("{{\n{entries}\n}}\n"));
    }
    Ok(files)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SOURCE: &str = "app inventory_dashboard\nprofile fullstack\ndata Product:\n  name: Text\n  price: Float\n  active: Bool\nresource products: Product:\n  list\n  create\n  delete\npage Dashboard at \"/\":\n  resource products\n";

    #[test]
    fn parses_fullstack_language() {
        let spec = parse(SOURCE).unwrap();
        assert_eq!(
            spec.models["Product"].fields[1],
            ("price".into(), "Float".into())
        );
        assert_eq!(spec.resources[0].actions, ["list", "create", "delete"]);
        assert_eq!(spec.storage, "json");
    }

    #[test]
    fn generates_json_project() {
        let files = project_files(SOURCE).unwrap();
        assert!(files["server.mjs"].contains("/api/health"));
        assert!(files["server.mjs"].contains("randomUUID"));
        assert!(files["server.mjs"].contains("mutationQueue"));
        assert!(files["server.mjs"].contains("value.trim().length > 0"));
        assert_eq!(files["data/store.json"], "{\n  \"products\": []\n}\n");
    }

    #[test]
    fn generates_postgres_project() {
        let source = SOURCE
            .replace("profile fullstack", "profile fullstack\nstorage postgres")
            .replace("  delete", "  update\n  delete")
            .replace("  name: Text", "  updated_at: Time");
        let files = project_files(&source).unwrap();
        assert!(files["package.json"].contains("\"pg\""));
        assert!(files["migrations/001_initial.sql"].contains("\"updated_at\" TIMESTAMPTZ NOT NULL"));
        assert!(files["server.mjs"].contains("new Pool"));
        assert!(files["server.mjs"].contains("UPDATE ${identifier(resource.name)}"));
        assert!(files["src/App.jsx"].contains("setEditingId"));
    }

    #[test]
    fn rejects_invalid_model_references() {
        let error = parse("app bad\nprofile fullstack\nresource things: Missing:\n  list\npage Home at \"/\":\n  resource things\n").unwrap_err();
        assert!(error.message.contains("unknown data 'Missing'"));
    }
}
