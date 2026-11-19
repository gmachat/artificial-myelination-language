#!/usr/bin/env node
// Protocol fixture only. Its synthetic usage is not benchmark evidence.
let input = "";
for await (const chunk of process.stdin) input += chunk;
const request = JSON.parse(input);
if (request.protocol !== "aml-ai-adapter-1") throw new Error("unsupported protocol");

const task = request.task || { app: "inventory", model: "Product", resource: "products", page: "Inventory", fields: [
  { name: "name", type: "Text" }, { name: "quantity", type: "Int" }, { name: "active", type: "Bool" },
] };
const fields = task.fields.map((field) => `  ${field.name}: ${field.type}`).join("\n");
const amlValid = `app ${task.app}
profile fullstack

data ${task.model}:
${fields}

resource ${task.resource}: ${task.model}:
  list
  create
  update
  delete

page ${task.page} at "/":
  resource ${task.resource}
`;

const amlNeedsRepair = amlValid.replace("  delete\n", "");
const typescript = {
  "package.json": JSON.stringify({ scripts: { dev: "node server.mjs", build: "vite build", start: "node server.mjs" }, dependencies: { vite: "latest", react: "latest" } }, null, 2),
  "index.html": "<div id=\"root\"></div><script type=\"module\" src=\"/src/main.tsx\"></script>",
  "src/main.tsx": `import React from 'react'; const api='/api/${task.resource}'; export default function App(){return <main>${task.title || task.page} {api}</main>}`,
  "server.mjs": `import { randomUUID } from 'node:crypto'; import { writeFile } from 'node:fs/promises'; const route='/api/${task.resource}'; const fields=${JSON.stringify(task.fields.map((field) => field.name))}; const methods=['GET','POST','PUT','DELETE']; function validate(x){if(Object.keys(x).some(k=>!fields.includes(k))) throw new Error('unknown field'); return {...x,id:randomUUID()};} await writeFile('store.json','[]'); console.log(route,methods,validate);`,
};

const files = request.condition === "aml"
  ? { "main.aml": request.round === 1 ? amlNeedsRepair : amlValid }
  : typescript;
const response = {
  protocol: "aml-ai-adapter-1",
  model: "mock/not-a-real-model",
  usage: {
    input_tokens: request.condition === "aml" ? 100 : 180,
    output_tokens: request.condition === "aml" ? 40 : 120,
    reasoning_tokens: 0,
  },
  files,
};
process.stdout.write(JSON.stringify(response));

