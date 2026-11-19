"""Dependency-free AML Language Server Protocol implementation."""

from __future__ import annotations

import json
import sys
from typing import Any, BinaryIO
from urllib.parse import unquote, urlparse

from .compiler import compile_source
from .services import completion_items, diagnostic_for_error, format_source


def _read_message(stream: BinaryIO) -> dict[str, Any] | None:
    headers: dict[str, str] = {}
    while True:
        line = stream.readline()
        if not line:
            return None
        if line in {b"\r\n", b"\n"}:
            break
        key, value = line.decode("ascii").split(":", 1)
        headers[key.lower()] = value.strip()
    length = int(headers.get("content-length", "0"))
    return json.loads(stream.read(length).decode("utf-8"))


def _write_message(stream: BinaryIO, payload: dict[str, Any]) -> None:
    body = json.dumps(payload, separators=(",", ":")).encode("utf-8")
    stream.write(f"Content-Length: {len(body)}\r\n\r\n".encode("ascii") + body)
    stream.flush()


def _path(uri: str) -> str:
    parsed = urlparse(uri)
    return unquote(parsed.path) if parsed.scheme == "file" else uri


def _diagnostics(uri: str, source: str) -> list[dict[str, Any]]:
    try:
        compile_source(source, _path(uri))
        return []
    except Exception as error:
        item = diagnostic_for_error(_path(uri), error)
        line = max(0, item.line - 1)
        character = max(0, item.column - 1)
        return [
            {
                "range": {
                    "start": {"line": line, "character": character},
                    "end": {"line": line, "character": character + 1},
                },
                "severity": 1,
                "code": item.code,
                "source": "aml",
                "message": item.message,
            }
        ]


def serve(input_stream: BinaryIO | None = None, output_stream: BinaryIO | None = None) -> None:
    input_stream = input_stream or sys.stdin.buffer
    output_stream = output_stream or sys.stdout.buffer
    documents: dict[str, str] = {}
    while True:
        message = _read_message(input_stream)
        if message is None:
            return
        method = message.get("method")
        identifier = message.get("id")
        params = message.get("params") or {}
        if method == "initialize":
            _write_message(
                output_stream,
                {
                    "jsonrpc": "2.0",
                    "id": identifier,
                    "result": {
                        "capabilities": {
                            "textDocumentSync": 1,
                            "documentFormattingProvider": True,
                            "completionProvider": {"triggerCharacters": [" ", ":"]},
                        },
                        "serverInfo": {"name": "aml-lsp", "version": "0.8"},
                    },
                },
            )
        elif method == "shutdown":
            _write_message(output_stream, {"jsonrpc": "2.0", "id": identifier, "result": None})
        elif method == "exit":
            return
        elif method in {"textDocument/didOpen", "textDocument/didChange"}:
            document = params["textDocument"]
            uri = document["uri"]
            if method.endswith("didOpen"):
                source = document["text"]
            else:
                source = params["contentChanges"][-1]["text"]
            documents[uri] = source
            _write_message(
                output_stream,
                {
                    "jsonrpc": "2.0",
                    "method": "textDocument/publishDiagnostics",
                    "params": {"uri": uri, "diagnostics": _diagnostics(uri, source)},
                },
            )
        elif method == "textDocument/formatting":
            uri = params["textDocument"]["uri"]
            source = documents.get(uri, "")
            lines = source.splitlines()
            _write_message(
                output_stream,
                {
                    "jsonrpc": "2.0",
                    "id": identifier,
                    "result": [
                        {
                            "range": {
                                "start": {"line": 0, "character": 0},
                                "end": {"line": len(lines) + 1, "character": 0},
                            },
                            "newText": format_source(source),
                        }
                    ],
                },
            )
        elif method == "textDocument/completion":
            _write_message(
                output_stream,
                {"jsonrpc": "2.0", "id": identifier, "result": completion_items()},
            )
        elif identifier is not None:
            _write_message(
                output_stream,
                {
                    "jsonrpc": "2.0",
                    "id": identifier,
                    "error": {"code": -32601, "message": f"method not found: {method}"},
                },
            )

