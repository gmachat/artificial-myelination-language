#!/usr/bin/env node
// Optional adapter. The AML compiler itself has no Anthropic dependency.
let raw = "";
for await (const chunk of process.stdin) raw += chunk;
const request = JSON.parse(raw);
if (request.protocol !== "aml-ai-adapter-1") throw new Error("unsupported adapter protocol");
const apiKey = process.env.ANTHROPIC_API_KEY;
const model = process.env.ANTHROPIC_MODEL;
if (!apiKey) throw new Error("ANTHROPIC_API_KEY is required");
if (!model) throw new Error("ANTHROPIC_MODEL is required so benchmark runs pin an explicit model");

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
const response = await fetch("https://api.anthropic.com/v1/messages", {
  method: "POST",
  headers: {
    "x-api-key": apiKey,
    "anthropic-version": process.env.ANTHROPIC_VERSION || "2026-10-21",
    "content-type": "application/json",
  },
  body: JSON.stringify({
    model,
    max_tokens: Number(process.env.ANTHROPIC_MAX_TOKENS || 16000),
    messages: [{ role: "user", content: prompt }],
    output_config: { format: { type: "json_schema", schema } },
  }),
});
const data = await response.json();
if (!response.ok) throw new Error(`Anthropic API ${response.status}: ${JSON.stringify(data).slice(0, 2000)}`);
const text = data.content?.filter((item) => item.type === "text").map((item) => item.text).join("");
if (!text) throw new Error("Anthropic response contained no text block");
const generated = JSON.parse(text);
const usage = data.usage || {};
const inputTokens = (usage.input_tokens || 0) + (usage.cache_creation_input_tokens || 0) + (usage.cache_read_input_tokens || 0);
process.stdout.write(JSON.stringify({
  protocol: "aml-ai-adapter-1",
  model: data.model || model,
  usage: {
    input_tokens: inputTokens,
    output_tokens: usage.output_tokens || 0,
    reasoning_tokens: usage.output_tokens_details?.thinking_tokens || 0,
  },
  files: generated.files,
}));
