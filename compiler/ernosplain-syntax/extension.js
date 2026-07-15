// ErnosPlain Language Server Protocol — VS Code Extension
//
// This extension starts the Ernos compiler in LSP mode (`ernos lsp`)
// and connects VS Code to it for diagnostics, completions, hover, and
// go-to-definition support.
//
// Requires: the `ernos` binary on PATH, or configure `ernosplain.compilerPath`.

const { spawn } = require("child_process");
const path = require("path");

let client = null;
let serverProcess = null;

function activate(context) {
  const vscode = require("vscode");

  // Determine compiler path
  const config = vscode.workspace.getConfiguration("ernosplain");
  const compilerPath = config.get("compilerPath", "ernos");

  // Output channel for logging
  const outputChannel = vscode.window.createOutputChannel("ErnosPlain LSP");
  outputChannel.appendLine(`Starting ErnosPlain LSP server: ${compilerPath} lsp`);

  // Spawn the LSP server process
  serverProcess = spawn(compilerPath, ["lsp"], {
    stdio: ["pipe", "pipe", "pipe"],
  });

  if (!serverProcess || !serverProcess.pid) {
    outputChannel.appendLine("Failed to start ErnosPlain LSP server.");
    outputChannel.appendLine(`Tried: ${compilerPath} lsp`);
    outputChannel.appendLine(
      "Make sure the ernos binary is on PATH or set ernosplain.compilerPath"
    );
    vscode.window.showErrorMessage(
      "ErnosPlain: Failed to start language server. Check output for details."
    );
    return;
  }

  outputChannel.appendLine(`LSP server started (PID: ${serverProcess.pid})`);

  // Log stderr (debug output from the LSP server)
  serverProcess.stderr.on("data", (data) => {
    outputChannel.appendLine(data.toString().trim());
  });

  serverProcess.on("exit", (code) => {
    outputChannel.appendLine(`LSP server exited with code ${code}`);
  });

  // Simple LSP client implementation using raw JSON-RPC over stdio
  // For full-featured LSP, install vscode-languageclient.
  // This minimal client handles the basic protocol.

  let requestId = 1;
  const pendingRequests = new Map();
  let buffer = "";

  // Parse LSP messages from stdout
  serverProcess.stdout.on("data", (chunk) => {
    buffer += chunk.toString();

    while (true) {
      // Look for Content-Length header
      const headerEnd = buffer.indexOf("\r\n\r\n");
      if (headerEnd === -1) break;

      const header = buffer.substring(0, headerEnd);
      const match = header.match(/Content-Length:\s*(\d+)/i);
      if (!match) {
        buffer = buffer.substring(headerEnd + 4);
        continue;
      }

      const contentLength = parseInt(match[1], 10);
      const bodyStart = headerEnd + 4;
      if (buffer.length < bodyStart + contentLength) break;

      const body = buffer.substring(bodyStart, bodyStart + contentLength);
      buffer = buffer.substring(bodyStart + contentLength);

      try {
        const message = JSON.parse(body);
        handleServerMessage(message, vscode, outputChannel);
      } catch (e) {
        outputChannel.appendLine(`Failed to parse LSP message: ${e}`);
      }
    }
  });

  // Store diagnostics collection
  const diagnosticCollection =
    vscode.languages.createDiagnosticCollection("ernosplain");
  context.subscriptions.push(diagnosticCollection);

  function handleServerMessage(message, vscode, _outputChannel) {
    if (message.id !== undefined && pendingRequests.has(message.id)) {
      // Response to a request
      const { resolve } = pendingRequests.get(message.id);
      pendingRequests.delete(message.id);
      resolve(message.result);
    } else if (message.method === "textDocument/publishDiagnostics") {
      // Diagnostics notification
      const params = message.params;
      const uri = vscode.Uri.parse(params.uri);
      const diagnostics = (params.diagnostics || []).map((d) => {
        const range = new vscode.Range(
          new vscode.Position(d.range.start.line, d.range.start.character),
          new vscode.Position(d.range.end.line, d.range.end.character)
        );
        const diag = new vscode.Diagnostic(
          range,
          d.message,
          d.severity === 1
            ? vscode.DiagnosticSeverity.Error
            : d.severity === 2
            ? vscode.DiagnosticSeverity.Warning
            : vscode.DiagnosticSeverity.Information
        );
        diag.source = d.source || "ernosplain";
        return diag;
      });
      diagnosticCollection.set(uri, diagnostics);
    }
  }

  function sendRequest(method, params) {
    const id = requestId++;
    return new Promise((resolve) => {
      pendingRequests.set(id, { resolve });
      sendMessage({ jsonrpc: "2.0", id, method, params });
    });
  }

  function sendNotification(method, params) {
    sendMessage({ jsonrpc: "2.0", method, params });
  }

  function sendMessage(msg) {
    const json = JSON.stringify(msg);
    const header = `Content-Length: ${Buffer.byteLength(json)}\r\n\r\n`;
    serverProcess.stdin.write(header + json);
  }

  // Initialize the server
  sendRequest("initialize", {
    processId: process.pid,
    capabilities: {},
    rootUri: vscode.workspace.workspaceFolders
      ? vscode.workspace.workspaceFolders[0].uri.toString()
      : null,
  }).then((result) => {
    outputChannel.appendLine("LSP server initialized successfully.");
    sendNotification("initialized", {});

    // Open all already-open .ep files
    for (const doc of vscode.workspace.textDocuments) {
      if (doc.languageId === "ep") {
        sendNotification("textDocument/didOpen", {
          textDocument: {
            uri: doc.uri.toString(),
            languageId: "ep",
            version: doc.version,
            text: doc.getText(),
          },
        });
      }
    }
  });

  // Watch for document opens
  context.subscriptions.push(
    vscode.workspace.onDidOpenTextDocument((doc) => {
      if (doc.languageId === "ep") {
        sendNotification("textDocument/didOpen", {
          textDocument: {
            uri: doc.uri.toString(),
            languageId: "ep",
            version: doc.version,
            text: doc.getText(),
          },
        });
      }
    })
  );

  // Watch for document changes
  context.subscriptions.push(
    vscode.workspace.onDidChangeTextDocument((event) => {
      if (event.document.languageId === "ep") {
        sendNotification("textDocument/didChange", {
          textDocument: {
            uri: event.document.uri.toString(),
            version: event.document.version,
          },
          contentChanges: [{ text: event.document.getText() }],
        });
      }
    })
  );

  // Watch for document closes
  context.subscriptions.push(
    vscode.workspace.onDidCloseTextDocument((doc) => {
      if (doc.languageId === "ep") {
        sendNotification("textDocument/didClose", {
          textDocument: { uri: doc.uri.toString() },
        });
      }
    })
  );

  // Register completion provider
  context.subscriptions.push(
    vscode.languages.registerCompletionItemProvider(
      { language: "ep" },
      {
        provideCompletionItems(document, position) {
          return sendRequest("textDocument/completion", {
            textDocument: { uri: document.uri.toString() },
            position: {
              line: position.line,
              character: position.character,
            },
          }).then((result) => {
            if (!result || !result.items) return [];
            return result.items.map((item) => {
              const ci = new vscode.CompletionItem(
                item.label,
                item.kind === 14
                  ? vscode.CompletionItemKind.Keyword
                  : vscode.CompletionItemKind.Function
              );
              ci.detail = item.detail;
              ci.insertText = item.insertText || item.label;
              return ci;
            });
          });
        },
      }
    )
  );

  // Register hover provider
  context.subscriptions.push(
    vscode.languages.registerHoverProvider(
      { language: "ep" },
      {
        provideHover(document, position) {
          return sendRequest("textDocument/hover", {
            textDocument: { uri: document.uri.toString() },
            position: {
              line: position.line,
              character: position.character,
            },
          }).then((result) => {
            if (!result || !result.contents) return null;
            const md = new vscode.MarkdownString(result.contents.value);
            return new vscode.Hover(md);
          });
        },
      }
    )
  );

  // Register definition provider
  context.subscriptions.push(
    vscode.languages.registerDefinitionProvider(
      { language: "ep" },
      {
        provideDefinition(document, position) {
          return sendRequest("textDocument/definition", {
            textDocument: { uri: document.uri.toString() },
            position: {
              line: position.line,
              character: position.character,
            },
          }).then((result) => {
            if (!result || !result.uri) return null;
            const uri = vscode.Uri.parse(result.uri);
            const range = new vscode.Range(
              new vscode.Position(
                result.range.start.line,
                result.range.start.character
              ),
              new vscode.Position(
                result.range.end.line,
                result.range.end.character
              )
            );
            return new vscode.Location(uri, range);
          });
        },
      }
    )
  );
}

function deactivate() {
  if (serverProcess) {
    serverProcess.kill();
    serverProcess = null;
  }
}

module.exports = { activate, deactivate };
