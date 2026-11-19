# AML AI benchmark adapter protocol

The benchmark starts an adapter executable directly, without a shell. It sends
one JSON request on standard input and reads one JSON response from standard
output. Logs belong on standard error.

Request fields:

```json
{
  "protocol": "aml-ai-adapter-1",
  "task": {
    "id": "inventory",
    "model": "Product",
    "resource": "products",
    "fields": [{ "name": "name", "type": "Text" }]
  },
  "condition": "aml",
  "round": 1,
  "prompt": "complete model prompt",
  "previous_files": null,
  "feedback": null
}
```

`task` is the selected public task definition and lets generic test adapters
produce task-specific fixtures. On a repair round, `previous_files` contains
the last file object and `feedback` contains evaluator diagnostics. The adapter
must include the entire corrected file set in its response.

Response fields:

```json
{
  "protocol": "aml-ai-adapter-1",
  "model": "provider/model-version",
  "usage": {
    "input_tokens": 1000,
    "output_tokens": 500,
    "reasoning_tokens": 0
  },
  "files": {
    "main.aml": "app inventory\nprofile fullstack\n..."
  }
}
```

Token counts must come from the provider response when available. In this
protocol, `reasoning_tokens` is the reported subset of `output_tokens`; it is
shown separately but not added again to `total_tokens`. An adapter must label
estimates separately instead of presenting them as exact usage.

