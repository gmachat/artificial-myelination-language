const vscode = require("vscode");
const { LanguageClient, TransportKind } = require("vscode-languageclient/node");

let client;
function activate(context) {
  const command = vscode.workspace.getConfiguration("aml").get("server.path", "aml");
  const serverOptions = { command, args: ["lsp"], transport: TransportKind.stdio };
  const clientOptions = {
    documentSelector: [{ scheme: "file", language: "aml" }],
    synchronize: { fileEvents: vscode.workspace.createFileSystemWatcher("**/*.aml") },
  };
  client = new LanguageClient("aml", "AML Language Server", serverOptions, clientOptions);
  context.subscriptions.push(client.start());
}
async function deactivate() { if (client) await client.stop(); }
module.exports = { activate, deactivate };

