#![allow(missing_docs)]

use serde_json::{Value, json};
use std::collections::BTreeMap;
use std::io::{self, BufRead, BufReader, Write};
use std::thread;
use std::time::Duration;

fn main() -> io::Result<()> {
    eprintln!("mock-rust-analyzer: ready");
    let fail_shutdown = std::env::var_os("MOCK_SHUTDOWN_FAILURE").is_some();
    let fail_initialized = std::env::var_os("MOCK_INITIALIZED_FAILURE").is_some();
    let hang_on_exit = std::env::var_os("MOCK_HANG_ON_EXIT").is_some();
    let emit_extra_startup_progress = std::env::var_os("MOCK_EXTRA_STARTUP_PROGRESS").is_some();

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = stdout.lock();
    let mut cancelled = Vec::new();
    let mut notifications = Vec::new();
    let mut open_documents = BTreeMap::new();
    let mut closed_documents = Vec::new();
    let mut configuration_changes = Vec::new();
    let mut watched_file_changes = Vec::new();
    let mut reload_workspace_requests = 0u64;
    let mut rebuild_proc_macro_requests = 0u64;
    let mut shutdown_requested = false;
    let mut initialize_params = None;
    let mut initialized_received = false;
    let mut config_response = Value::Null;

    while let Some(message) = read_message(&mut reader)? {
        let method = message.get("method").and_then(Value::as_str);
        let id = message.get("id").cloned();

        match (method, id) {
            (Some("initialize"), Some(id)) => {
                initialize_params = message.get("params").cloned();
                write_message(
                    &mut writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "capabilities": {
                                "completionProvider": {
                                    "resolveProvider": false,
                                    "triggerCharacters": [".", ":"]
                                },
                                "definitionProvider": true,
                                "documentSymbolProvider": true,
                                "hoverProvider": true,
                                "positionEncoding": "utf-8",
                                "referencesProvider": true,
                                "renameProvider": {
                                    "prepareProvider": true
                                },
                                "workspaceSymbolProvider": true
                            },
                            "serverInfo": {
                                "name": "mock-rust-analyzer",
                                "version": "0.0.0"
                            }
                        }
                    }),
                )?;
            }
            (Some("ping"), Some(id)) => {
                let params = message.get("params").cloned().unwrap_or(Value::Null);
                write_message(
                    &mut writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": { "echo": params }
                    }),
                )?;
            }
            (Some("slow_ping"), Some(id)) => {
                thread::sleep(Duration::from_millis(200));
                let params = message.get("params").cloned().unwrap_or(Value::Null);
                write_message(
                    &mut writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": { "echo": params }
                    }),
                )?;
            }
            (Some("textDocument/hover"), Some(id)) => {
                let uri = text_document_uri(&message);
                write_message(
                    &mut writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "contents": {
                                "kind": "markdown",
                                "value": "```rust\nfn answer() -> u32\n```"
                            },
                            "range": hover_range()
                        }
                    }),
                )?;
                let _ = uri;
            }
            (Some("textDocument/completion"), Some(id)) => {
                write_message(
                    &mut writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "isIncomplete": false,
                            "items": [
                                {
                                    "label": "answer",
                                    "kind": 3,
                                    "detail": "fn answer() -> u32",
                                    "documentation": {
                                        "kind": "markdown",
                                        "value": "Returns the answer."
                                    },
                                    "insertText": "answer"
                                }
                            ]
                        }
                    }),
                )?;
            }
            (Some("textDocument/definition"), Some(id)) => {
                let uri = text_document_uri(&message);
                write_message(
                    &mut writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": [
                            {
                                "uri": uri,
                                "range": symbol_range()
                            }
                        ]
                    }),
                )?;
            }
            (Some("textDocument/references"), Some(id)) => {
                let uri = text_document_uri(&message);
                write_message(
                    &mut writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": [
                            {
                                "uri": uri,
                                "range": symbol_range()
                            },
                            {
                                "uri": uri,
                                "range": {
                                    "start": { "line": 5, "character": 4 },
                                    "end": { "line": 5, "character": 10 }
                                }
                            }
                        ]
                    }),
                )?;
            }
            (Some("textDocument/prepareRename"), Some(id)) => {
                write_message(
                    &mut writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "range": symbol_range(),
                            "placeholder": "answer"
                        }
                    }),
                )?;
            }
            (Some("textDocument/rename"), Some(id)) => {
                let uri = text_document_uri(&message);
                let new_name = message
                    .get("params")
                    .and_then(|params| params.get("newName"))
                    .cloned()
                    .unwrap_or_else(|| json!("renamed"));
                let mut changes = serde_json::Map::new();
                changes.insert(
                    uri,
                    Value::Array(vec![json!({
                        "range": symbol_range(),
                        "newText": new_name
                    })]),
                );
                write_message(
                    &mut writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "changes": changes
                        }
                    }),
                )?;
            }
            (Some("textDocument/documentSymbol"), Some(id)) => {
                write_message(
                    &mut writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": [
                            {
                                "name": "answer",
                                "detail": "fn() -> u32",
                                "kind": 12,
                                "range": {
                                    "start": { "line": 1, "character": 0 },
                                    "end": { "line": 3, "character": 1 }
                                },
                                "selectionRange": symbol_range(),
                                "children": []
                            }
                        ]
                    }),
                )?;
            }
            (Some("workspace/symbol"), Some(id)) => {
                let query = message
                    .get("params")
                    .and_then(|params| params.get("query"))
                    .cloned()
                    .unwrap_or_else(|| json!(""));
                let query = query.as_str().unwrap_or_default();
                write_message(
                    &mut writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": [
                            {
                                "name": query,
                                "kind": 12,
                                "location": {
                                    "uri": format!("file:///workspace/{query}.rs"),
                                    "range": symbol_range()
                                },
                                "containerName": "crate"
                            }
                        ]
                    }),
                )?;
            }
            (Some("rust-analyzer/analyzerStatus"), Some(id)) => {
                let uri = message
                    .get("params")
                    .and_then(|params| params.get("textDocument"))
                    .and_then(|text_document| text_document.get("uri"))
                    .and_then(Value::as_str)
                    .unwrap_or("workspace");
                write_message(
                    &mut writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": format!("status:{uri}")
                    }),
                )?;
            }
            (Some("rust-analyzer/fetchDependencyList"), Some(id)) => {
                write_message(
                    &mut writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "crates": [
                                {
                                    "name": "serde",
                                    "version": "1.0.0",
                                    "path": "file:///deps/serde/Cargo.toml"
                                }
                            ]
                        }
                    }),
                )?;
            }
            (Some("rust-analyzer/reloadWorkspace"), Some(id)) => {
                reload_workspace_requests += 1;
                write_message(
                    &mut writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": null
                    }),
                )?;
            }
            (Some("rust-analyzer/rebuildProcMacros"), Some(id)) => {
                rebuild_proc_macro_requests += 1;
                write_message(
                    &mut writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": null
                    }),
                )?;
            }
            (Some("rust-analyzer/viewSyntaxTree"), Some(id)) => {
                let uri = text_document_uri(&message);
                write_message(
                    &mut writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": format!("syntax tree for {uri}")
                    }),
                )?;
            }
            (Some("rust-analyzer/viewHir"), Some(id)) => {
                write_message(
                    &mut writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": "hir debug output"
                    }),
                )?;
            }
            (Some("rust-analyzer/viewMir"), Some(id)) => {
                write_message(
                    &mut writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": "mir debug output"
                    }),
                )?;
            }
            (Some("rust-analyzer/expandMacro"), Some(id)) => {
                write_message(
                    &mut writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "name": "println!",
                            "expansion": "std::io::_print(format_args!(\"hi\"))"
                        }
                    }),
                )?;
            }
            (Some("experimental/runnables"), Some(id)) => {
                write_message(
                    &mut writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": [
                            {
                                "label": "cargo test answer",
                                "kind": "cargo",
                                "args": {
                                    "cargoArgs": ["test", "answer"],
                                    "executableArgs": ["--exact"],
                                    "cwd": "/workspace",
                                    "workspaceRoot": "/workspace",
                                    "environment": {}
                                }
                            }
                        ]
                    }),
                )?;
            }
            (Some("rust-analyzer/relatedTests"), Some(id)) => {
                write_message(
                    &mut writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": [
                            {
                                "runnable": {
                                    "label": "cargo test related_case",
                                    "kind": "cargo",
                                    "args": {
                                        "cargoArgs": ["test", "related_case"],
                                        "executableArgs": [],
                                        "cwd": "/workspace",
                                        "workspaceRoot": "/workspace",
                                        "environment": {}
                                    }
                                }
                            }
                        ]
                    }),
                )?;
            }
            (Some("server_request"), Some(id)) => {
                write_message(
                    &mut writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "method": "workspace/configuration",
                        "id": "config-1",
                        "params": { "items": [] }
                    }),
                )?;
                write_message(
                    &mut writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": { "status": "request-sent" }
                    }),
                )?;
            }
            (Some("state"), Some(id)) => {
                write_message(
                    &mut writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "cancelled": cancelled,
                            "closed_documents": closed_documents,
                            "configuration_changes": configuration_changes,
                            "config_response": config_response,
                            "initialize_params": initialize_params,
                            "initialized_received": initialized_received,
                            "notifications": notifications,
                            "open_documents": open_documents,
                            "rebuild_proc_macro_requests": rebuild_proc_macro_requests,
                            "reload_workspace_requests": reload_workspace_requests,
                            "shutdown_requested": shutdown_requested
                            ,
                            "watched_file_changes": watched_file_changes
                        }
                    }),
                )?;
            }
            (Some("shutdown"), Some(id)) => {
                if fail_shutdown {
                    let _ = id;
                    break;
                }
                shutdown_requested = true;
                write_message(
                    &mut writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": null
                    }),
                )?;
            }
            (Some("exit"), None) => {
                if hang_on_exit {
                    loop {
                        thread::sleep(Duration::from_secs(1));
                    }
                }
                break;
            }
            (Some("initialized"), None) => {
                if fail_initialized {
                    break;
                }
                if emit_extra_startup_progress {
                    write_message(
                        &mut writer,
                        &json!({
                            "jsonrpc": "2.0",
                            "method": "$/progress",
                            "params": {
                                "token": "rustAnalyzer/cargo",
                                "value": {
                                    "kind": "end",
                                    "message": "Cargo metadata complete"
                                }
                            }
                        }),
                    )?;
                }
                initialized_received = true;
                record_notification(&mut writer, &mut notifications, "initialized")?;
                write_message(
                    &mut writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "method": "workspace/configuration",
                        "id": "config-1",
                        "params": {
                            "items": [
                                { "section": "rust-analyzer" },
                                { "section": "rust-analyzer.procMacro" }
                            ]
                        }
                    }),
                )?;
                // Register the progress token via window/workDoneProgress/create
                // so the client tracks it in registered_progress_tokens.
                write_message(
                    &mut writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "method": "window/workDoneProgress/create",
                        "id": "progress-create-workspace",
                        "params": {
                            "token": "rustAnalyzer/workspace"
                        }
                    }),
                )?;
                write_message(
                    &mut writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "method": "$/progress",
                        "params": {
                            "token": "rustAnalyzer/workspace",
                            "value": {
                                "kind": "begin",
                                "title": "Indexing",
                                "message": "Loading workspace"
                            }
                        }
                    }),
                )?;
                write_message(
                    &mut writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "method": "$/progress",
                        "params": {
                            "token": "rustAnalyzer/workspace",
                            "value": {
                                "kind": "end",
                                "message": "Workspace ready"
                            }
                        }
                    }),
                )?;
            }
            (Some("$/cancelRequest"), None) => {
                if let Some(id) = message
                    .get("params")
                    .and_then(|params| params.get("id"))
                    .cloned()
                {
                    cancelled.push(id);
                }
            }
            (Some("textDocument/didOpen"), None) => {
                record_notification(&mut writer, &mut notifications, "textDocument/didOpen")?;
                if let Some(document) = message
                    .get("params")
                    .and_then(|params| params.get("textDocument"))
                    && let Some(uri) = document.get("uri").and_then(Value::as_str)
                {
                    open_documents.insert(uri.to_owned(), document.clone());
                }
            }
            (Some("textDocument/didChange"), None) => {
                record_notification(&mut writer, &mut notifications, "textDocument/didChange")?;
                if let Some(params) = message.get("params") {
                    let uri = params
                        .get("textDocument")
                        .and_then(|document| document.get("uri"))
                        .and_then(Value::as_str);
                    let version = params
                        .get("textDocument")
                        .and_then(|document| document.get("version"))
                        .cloned();
                    let text = params
                        .get("contentChanges")
                        .and_then(Value::as_array)
                        .and_then(|changes| changes.last())
                        .and_then(|change| change.get("text"))
                        .cloned();

                    if let (Some(uri), Some(version), Some(text)) = (uri, version, text)
                        && let Some(document) = open_documents.get_mut(uri)
                    {
                        document["version"] = version;
                        document["text"] = text;
                    }
                }
            }
            (Some("textDocument/didSave"), None) => {
                record_notification(&mut writer, &mut notifications, "textDocument/didSave")?;
            }
            (Some("textDocument/didClose"), None) => {
                record_notification(&mut writer, &mut notifications, "textDocument/didClose")?;
                if let Some(uri) = message
                    .get("params")
                    .and_then(|params| params.get("textDocument"))
                    .and_then(|document| document.get("uri"))
                    .and_then(Value::as_str)
                {
                    open_documents.remove(uri);
                    closed_documents.push(uri.to_owned());
                }
            }
            (Some("workspace/didChangeConfiguration"), None) => {
                record_notification(
                    &mut writer,
                    &mut notifications,
                    "workspace/didChangeConfiguration",
                )?;
                if let Some(params) = message.get("params") {
                    configuration_changes.push(params.clone());
                }
            }
            (Some("workspace/didChangeWatchedFiles"), None) => {
                record_notification(
                    &mut writer,
                    &mut notifications,
                    "workspace/didChangeWatchedFiles",
                )?;
                if let Some(changes) = message
                    .get("params")
                    .and_then(|params| params.get("changes"))
                    .and_then(Value::as_array)
                {
                    watched_file_changes.extend(changes.iter().cloned());
                }
            }
            (None, Some(id)) if id == json!("config-1") => {
                config_response = message.get("result").cloned().unwrap_or(Value::Null);
            }
            (None, Some(id)) if id == json!("progress-create-workspace") => {
                // Response to window/workDoneProgress/create — nothing to store.
            }
            (Some(method), None) => {
                record_notification(&mut writer, &mut notifications, method)?;
            }
            _ => {}
        }

        thread::sleep(Duration::from_millis(10));
    }

    Ok(())
}

fn read_message(reader: &mut impl BufRead) -> io::Result<Option<Value>> {
    let mut content_length = None;

    loop {
        let mut line = String::new();
        let bytes = reader.read_line(&mut line)?;
        if bytes == 0 {
            return Ok(None);
        }

        if line == "\r\n" {
            break;
        }

        let (name, value) = line
            .split_once(':')
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "malformed header"))?;

        if name.eq_ignore_ascii_case("content-length") {
            content_length = Some(
                value
                    .trim()
                    .parse::<usize>()
                    .map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "invalid length"))?,
            );
        }
    }

    let content_length = content_length
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidData, "missing length"))?;
    let mut body = vec![0; content_length];
    reader.read_exact(&mut body)?;
    serde_json::from_slice(&body)
        .map_err(io::Error::other)
        .map(Some)
}

fn write_message(writer: &mut impl Write, message: &Value) -> io::Result<()> {
    let body = serde_json::to_vec(message).map_err(io::Error::other)?;
    write!(writer, "Content-Length: {}\r\n\r\n", body.len())?;
    writer.write_all(&body)?;
    writer.flush()
}

fn record_notification(
    writer: &mut impl Write,
    notifications: &mut Vec<String>,
    method: &str,
) -> io::Result<()> {
    notifications.push(method.to_owned());
    write_message(
        writer,
        &json!({
            "jsonrpc": "2.0",
            "method": "$/progress",
            "params": {
                "token": "mock-progress",
                "value": {
                    "kind": "report",
                    "message": format!("saw:{method}")
                }
            }
        }),
    )
}

fn text_document_uri(message: &Value) -> String {
    message
        .get("params")
        .and_then(|params| params.get("textDocument"))
        .and_then(|document| document.get("uri"))
        .and_then(Value::as_str)
        .unwrap_or("file:///workspace/src/lib.rs")
        .to_owned()
}

fn hover_range() -> Value {
    json!({
        "start": { "line": 1, "character": 3 },
        "end": { "line": 1, "character": 9 }
    })
}

fn symbol_range() -> Value {
    json!({
        "start": { "line": 1, "character": 3 },
        "end": { "line": 1, "character": 9 }
    })
}
