# AI integration

AML's integration boundary is provider-neutral. The compiler does not call,
embed, or require ChatGPT, Claude, Copilot, or another model service.

## Create a project

```sh
aml new hello
aml new dashboard --profile react
aml new website --profile web
aml new inventory --profile fullstack
```

`aml new` refuses to overwrite a nonempty directory. Each project contains a
package manifest and canonical `main.aml` entrypoint, reducing navigation and
file-layout decisions for coding agents.

## Produce an agent context packet

From a project directory:

```sh
aml agent context --json > aml-context.json
```

The packet uses schema `aml-agent-context-1` and contains:

- the compact AML language reference;
- AML source files and package manifests in deterministic order;
- canonical check, format, build, and run commands;
- editing constraints;
- a transparent lexical token estimate.

Only `.aml`, `aml.package.json`, and `aml.lock.json` files are eligible. AML
skips symlinks, hidden secrets such as `.env`, dependency directories, and
generated output. Context is capped at 50 files and 64 KiB so accidental prompt
growth fails visibly rather than silently consuming tokens.

Give the JSON packet and the user's task to any capable coding model. Apply its
file changes, then use `aml check main.aml --json`. Provider adapters and an MCP
server can automate those steps later without changing AML's language core.

