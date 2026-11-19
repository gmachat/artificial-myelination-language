#!/usr/bin/env node
// Optional adapter. The AML compiler itself has no OpenAI dependency.
let raw = "";
for await (const chunk of process.stdin) raw += chunk;
const request = JSON.parse(raw);
if (request.protocol !== "aml-ai-adapter-1") throw new Error("unsupported adapter protocol");
const apiKey = process.env.OPENAI_API_KEY;
const model = process.env.OPENAI_MODEL;
if (!apiKey) throw new Error("OPENAI_API_KEY is required");
if (!model) throw new Error("OPENAI_MODEL is required so benchmark runs pin an explicit model");

let prompt = request.prompt;
if (request.previous_files) prompt += `\n\nPrevious complete file set:\n${JSON.stringify({ files: request.previous_files })}`;
if (request.feedback) prompt += `\n\nEvaluator feedback to repair:\n${JSON.stringify(request.feedback)}`;
prompt += "\n\nReturn the entire corrected file set as the required JSON object.";
const schema = {
  type: "object",
  properties: { files: { type: "object", additionalProperties: { type: "string" } } },
  required: ["files"],
  additionalProperties: false,
};
const response = await fetch("https://api.openai.com/v1/responses", {
  method: "POST",
  headers: { Authorization: `Bearer ${apiKey}`, "Content-Type": "application/json" },
  body: JSON.stringify({
    model,
    store: false,
    input: [{ role: "user", content: [{ type: "input_text", text: prompt }] }],
    max_output_tokens: Number(process.env.OPENAI_MAX_OUTPUT_TOKENS || 16000),
    text: { format: { type: "json_schema", name: "aml_benchmark_files", strict: true, schema } },
  }),
});
const data = await response.json();
if (!response.ok) throw new Error(`OpenAI API ${response.status}: ${JSON.stringify(data).slice(0, 2000)}`);
const text = data.output?.flatMap((item) => item.type === "message" ? item.content || [] : [])
  .filter((item) => item.type === "output_text").map((item) => item.text).join("") || data.output_text;
if (!text) throw new Error("OpenAI response contained no output text");
const generated = JSON.parse(text);
const usage = data.usage || {};
process.stdout.write(JSON.stringify({
  protocol: "aml-ai-adapter-1",
  model: data.model || model,
  usage: {
    input_tokens: usage.input_tokens || 0,
    output_tokens: usage.output_tokens || 0,
    reasoning_tokens: usage.output_tokens_details?.reasoning_tokens || 0,
  },
  files: generated.files,
}));

