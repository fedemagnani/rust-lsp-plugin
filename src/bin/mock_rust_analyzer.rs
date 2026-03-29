#![allow(missing_docs)]

use serde_json::{json, Value};
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
                                "hoverProvider": true,
                                "positionEncoding": "utf-8"
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
                            "config_response": config_response,
                            "initialize_params": initialize_params,
                            "initialized_received": initialized_received,
                            "notifications": notifications,
                            "shutdown_requested": shutdown_requested
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
                notifications.push("initialized".to_owned());
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
                write_message(
                    &mut writer,
                    &json!({
                        "jsonrpc": "2.0",
                        "method": "$/progress",
                        "params": {
                            "token": "rustAnalyzer/workspace",
                            "value": {
                                "kind": "begin",
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
            (None, Some(id)) if id == json!("config-1") => {
                config_response = message.get("result").cloned().unwrap_or(Value::Null);
            }
            (Some(method), None) => {
                notifications.push(method.to_owned());
                write_message(
                    &mut writer,
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
                )?;
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
