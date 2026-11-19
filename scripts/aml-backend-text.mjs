#!/usr/bin/env node

let source = "";
for await (const chunk of process.stdin) source += chunk;
const request = JSON.parse(source);

if (request.protocol !== "aml-backend-1") {
  process.stderr.write("unsupported AML backend protocol\n");
  process.exit(1);
}

const app = request.ir.app;
process.stdout.write(JSON.stringify({
  protocol: "aml-backend-1",
  entry: "main.txt",
  artifacts: [{ path: "main.txt", content: `app=${app}\n` }],
  diagnostics: [],
}));
