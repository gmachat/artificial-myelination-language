# AML MCP server

AML includes a native, provider-neutral Model Context Protocol server. It uses
stdio with newline-delimited JSON-RPC and requires no Node or Python runtime.

## Configure a host

Generate a configuration using absolute executable and project paths:

```sh
aml mcp --project /absolute/path/to/project --print-config
```

The output has the common `mcpServers` shape:

```json
{
  "mcpServers": {
    "aml": {
      "args": ["mcp", "--project", "/absolute/path/to/project"],
      "command": "/absolute/path/to/aml"
    }
  }
}
```

Add the generated `aml` entry wherever the AI host accepts local MCP server
configuration. The host launches the process; do not manually keep `aml mcp`
running in another terminal.

## Tools

- `aml_project_context`: compact project context and language reference.
- `aml_check`: structured diagnostics for one AML file.
- `aml_format`: canonical in-place formatting for one AML file.
- `aml_build`: compilation to an explicitly named project output.
- `aml_diagnose`: categorized diagnostics with a small source window.
- `aml_repair_context`: minimal context for repairing one AML file.
- `aml_test`: isolated recursive AML test execution.
- `aml_doctor`: local target-tool discovery.
- `aml_targets`: target and platform metadata.

Read-only and writing tools carry MCP annotations so hosts can distinguish
them. A failed tool returns `isError: true` with text an agent can use for a
repair attempt.

## Compatibility and security

The server supports the `2026-07-28` stateless discovery lifecycle and the
legacy `2025-11-25` initialization lifecycle. CI exercises both.

`--project` is the security boundary. Source and output paths must be relative,
remain beneath the canonical project root, and cannot traverse symbolic links.
The context tool includes only AML source and AML manifests. `aml_format` and
`aml_build` intentionally write within that root; hosts should retain their
normal tool-approval policy for writing calls.

Maintainers can run the protocol test directly:

```sh
cargo build --release
node scripts/mcp-smoke.mjs
```

