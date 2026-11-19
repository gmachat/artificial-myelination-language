import assert from "node:assert/strict";
import test from "node:test";
import { parseEnvironment } from "./openai_runner.mjs";

test("OpenAI environment parser accepts only expected settings", () => {
  const values = parseEnvironment("# local\nOPENAI_API_KEY='secret'\nOPENAI_MODEL=gpt-test\nOPENAI_MAX_OUTPUT_TOKENS=16000\n");
  assert.deepEqual(values, { OPENAI_API_KEY: "secret", OPENAI_MODEL: "gpt-test", OPENAI_MAX_OUTPUT_TOKENS: "16000" });
  assert.throws(() => parseEnvironment("OTHER_SECRET=nope\n"), /unsupported setting/);
});
