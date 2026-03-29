#![allow(missing_docs)]

use serde_json::{json, Value};
use std::io::{self, BufRead, BufReader, Write};
use std::thread;
use std::time::Duration;

fn main() -> io::Result<()> {
    eprintln!("mock-rust-analyzer: ready");

    let stdin = io::stdin();
    let stdout = io::stdout();
    let mut reader = BufReader::new(stdin.lock());
    let mut writer = stdout.lock();
    let mut cancelled = Vec::new();
    let mut notifications = Vec::new();
    let mut shutdown_requested = false;

    while let Some(message) = read_message(&mut reader)? {
        let method = message.get("method").and_then(Value::as_str);
        let id = message.get("id").cloned();

        match (method, id) {
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
                            "notifications": notifications,
                            "shutdown_requested": shutdown_requested
                        }
                    }),
                )?;
            }
            (Some("shutdown"), Some(id)) => {
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
            (Some("exit"), None) => break,
            (Some("$/cancelRequest"), None) => {
                if let Some(id) = message
                    .get("params")
                    .and_then(|params| params.get("id"))
                    .cloned()
                {
                    cancelled.push(id);
                }
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
