#![allow(missing_docs)]

use serde_json::{Value, json};
use std::error::Error;
use std::io::{self, BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant};

#[test]
fn mcp_server_starts_serves_requests_and_exits_when_stdio_closes() -> Result<(), Box<dyn Error>> {
    let mut child = spawn_server()?;
    let mut stdin = child.stdin.take().expect("server stdin");
    let stdout = child.stdout.take().expect("server stdout");
    let mut stdout = BufReader::new(stdout);

    write_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "runtime-test",
                    "version": "0.0.0"
                }
            }
        }),
    )?;

    let initialize_response = read_message(&mut stdout)?.expect("initialize response");
    assert_eq!(initialize_response["id"], json!(1));
    assert!(
        initialize_response["result"]["serverInfo"]["name"]
            .as_str()
            .is_some_and(|name| !name.is_empty()),
        "server should report a non-empty server name"
    );
    assert!(
        initialize_response["result"]["capabilities"]["tools"].is_object(),
        "server should advertise tool capability"
    );

    write_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }),
    )?;

    write_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list"
        }),
    )?;

    let tools_response = read_message(&mut stdout)?.expect("tools list response");
    assert_eq!(tools_response["id"], json!(2));
    let tools = tools_response["result"]["tools"]
        .as_array()
        .expect("tools list should be an array");
    assert!(
        !tools.is_empty(),
        "server should expose the registered read-only tools"
    );

    let tool_names = tools
        .iter()
        .filter_map(|tool| tool["name"].as_str())
        .collect::<Vec<_>>();
    let mut tool_names = tool_names;
    tool_names.sort_unstable();

    let mut expected = vec![
        "definitions",
        "analyzer_status",
        "view_syntax_tree",
        "references",
        "hover",
        "workspace_symbols",
    ];
    expected.sort_unstable();
    assert_eq!(
        tool_names,
        expected
    );
    assert!(child.try_wait()?.is_none(), "server exited before stdio closed");

    drop(stdin);
    wait_for_exit(&mut child, Duration::from_secs(2))?;

    Ok(())
}

fn spawn_server() -> Result<Child, io::Error> {
    let binary = env!("CARGO_BIN_EXE_rust-lsp-mcp");
    Command::new(binary)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .spawn()
}

fn wait_for_exit(child: &mut Child, timeout: Duration) -> Result<(), Box<dyn Error>> {
    let start = Instant::now();
    loop {
        if let Some(status) = child.try_wait()? {
            if status.success() {
                return Ok(());
            }
            return Err(format!("server exited with status {status}").into());
        }

        if start.elapsed() >= timeout {
            return Err("timed out waiting for server shutdown".into());
        }

        thread::sleep(Duration::from_millis(10));
    }
}

fn read_message(reader: &mut BufReader<ChildStdout>) -> io::Result<Option<Value>> {
    let mut line = String::new();
    let bytes = reader.read_line(&mut line)?;
    if bytes == 0 {
        return Ok(None);
    }

    let line = line.trim_end_matches(['\r', '\n']);
    let message = serde_json::from_str(line).map_err(io::Error::other)?;
    Ok(Some(message))
}

fn write_message(stdin: &mut ChildStdin, message: &Value) -> io::Result<()> {
    serde_json::to_writer(&mut *stdin, message).map_err(io::Error::other)?;
    stdin.write_all(b"\n")?;
    stdin.flush()
}
