# AML for VS Code

Install the native `aml` executable first, then run `npm install` and
`npm run package` in this directory to create a `.vsix`. Install that package
through VS Code's **Extensions: Install from VSIX** command.

The extension starts `aml lsp` over standard I/O. If AML is not on `PATH`, set
`aml.server.path` to the absolute executable path. It provides AML syntax
highlighting plus diagnostics, formatting, hover, and completion capabilities
advertised by the native language server.
