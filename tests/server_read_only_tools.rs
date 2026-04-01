#![allow(missing_docs)]

use serde_json::{Value, json};
use std::error::Error;
use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

#[test]
fn mcp_server_exposes_representative_read_only_analysis_tools() -> Result<(), Box<dyn Error>> {
    let workspace_root = create_temp_workspace("read-only-tools");
    let file_path = workspace_root.join("src").join("lib.rs");
    fs::create_dir_all(file_path.parent().expect("src dir"))?;
    fs::write(
        &file_path,
        "pub fn answer() -> u32 {\n    42\n}\n\npub fn use_answer() -> u32 {\n    answer()\n}\n",
    )?;

    let mut child = spawn_server(&workspace_root)?;
    let mut stdin = child.stdin.take().expect("server stdin");
    let stdout = child.stdout.take().expect("server stdout");
    let mut stdout = BufReader::new(stdout);

    initialize_server(&mut stdin, &mut stdout)?;

    write_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/list"
        }),
    )?;
    let tools_response = read_message(&mut stdout)?.expect("tools list response");
    let tools = tools_response["result"]["tools"]
        .as_array()
        .expect("tools array");
    assert!(tools.iter().any(|tool| tool["name"] == "hover"));
    assert!(tools.iter().any(|tool| tool["name"] == "definitions"));
    assert!(tools.iter().any(|tool| tool["name"] == "references"));
    assert!(tools.iter().any(|tool| tool["name"] == "workspace_symbols"));
    assert!(tools.iter().any(|tool| tool["name"] == "analyzer_status"));
    assert!(tools.iter().any(|tool| tool["name"] == "view_syntax_tree"));
    let read_only_names: Vec<&str> = vec![
        "hover",
        "definitions",
        "references",
        "workspace_symbols",
        "analyzer_status",
        "view_syntax_tree",
    ];
    let read_only_tools: Vec<_> = tools
        .iter()
        .filter(|tool| read_only_names.contains(&tool["name"].as_str().unwrap_or("")))
        .collect();
    assert_eq!(read_only_tools.len(), read_only_names.len());
    assert!(read_only_tools.iter().all(|tool| {
        tool["annotations"]["readOnlyHint"] == json!(true)
            && tool["annotations"]["destructiveHint"] == json!(false)
    }));

    let hover_result = call_tool(
        &mut stdin,
        &mut stdout,
        3,
        "hover",
        json!({
            "workspace_root": workspace_root,
            "document_path": file_path,
            "position": { "line": 5, "character": 4 }
        }),
    )?;
    assert_eq!(
        hover_result["structuredContent"]["data"]["contents"]["kind"],
        json!("markdown")
    );
    assert!(
        hover_result["structuredContent"]["data"]["contents"]["value"]
            .as_str()
            .expect("hover value")
            .contains("fn answer")
    );

    let definitions_result = call_tool(
        &mut stdin,
        &mut stdout,
        4,
        "definitions",
        json!({
            "workspace_root": workspace_root,
            "document_path": file_path,
            "position": { "line": 5, "character": 4 }
        }),
    )?;
    assert!(
        definitions_result["structuredContent"]["data"][0]["document_path"]
            .as_str()
            .expect("definition path")
            .ends_with("/src/lib.rs")
    );

    let references_result = call_tool(
        &mut stdin,
        &mut stdout,
        5,
        "references",
        json!({
            "workspace_root": workspace_root,
            "document_path": file_path,
            "position": { "line": 5, "character": 4 }
        }),
    )?;
    assert_eq!(
        references_result["structuredContent"]["data"]
            .as_array()
            .unwrap()
            .len(),
        2
    );

    let symbols_result = call_tool(
        &mut stdin,
        &mut stdout,
        6,
        "workspace_symbols",
        json!({
            "workspace_root": workspace_root,
            "query": "answer"
        }),
    )?;
    assert_eq!(
        symbols_result["structuredContent"]["data"][0]["name"],
        json!("answer")
    );
    assert_eq!(
        symbols_result["structuredContent"]["data"][0]["container_name"],
        json!("crate")
    );

    let analyzer_status = call_tool(
        &mut stdin,
        &mut stdout,
        7,
        "analyzer_status",
        json!({
            "workspace_root": workspace_root,
            "document_path": file_path
        }),
    )?;
    assert!(
        analyzer_status["structuredContent"]["data"]["status"]
            .as_str()
            .expect("status string")
            .starts_with("status:file://")
    );

    let workspace_status = call_tool(
        &mut stdin,
        &mut stdout,
        8,
        "analyzer_status",
        json!({
            "workspace_root": workspace_root
        }),
    )?;
    assert_eq!(
        workspace_status["structuredContent"]["data"]["status"],
        json!("status:workspace")
    );

    let syntax_tree = call_tool(
        &mut stdin,
        &mut stdout,
        9,
        "view_syntax_tree",
        json!({
            "workspace_root": workspace_root,
            "document_path": file_path
        }),
    )?;
    assert!(
        syntax_tree["structuredContent"]["data"]["tree"]
            .as_str()
            .expect("tree string")
            .contains("syntax tree for file://")
    );

    drop(stdin);
    wait_for_exit(&mut child, Duration::from_secs(2))?;
    remove_temp_workspace(&workspace_root);
    Ok(())
}

fn initialize_server(
    stdin: &mut ChildStdin,
    stdout: &mut BufReader<ChildStdout>,
) -> Result<(), Box<dyn Error>> {
    write_message(
        stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "initialize",
            "params": {
                "protocolVersion": "2024-11-05",
                "capabilities": {},
                "clientInfo": {
                    "name": "server-read-only-tools-test",
                    "version": "0.0.0"
                }
            }
        }),
    )?;
    let initialize_response = read_message(stdout)?.expect("initialize response");
    assert_eq!(initialize_response["id"], json!(1));

    write_message(
        stdin,
        &json!({
            "jsonrpc": "2.0",
            "method": "notifications/initialized"
        }),
    )?;

    Ok(())
}

fn call_tool(
    stdin: &mut ChildStdin,
    stdout: &mut BufReader<ChildStdout>,
    id: u64,
    name: &str,
    arguments: Value,
) -> Result<Value, Box<dyn Error>> {
    write_message(
        stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": "tools/call",
            "params": {
                "name": name,
                "arguments": arguments
            }
        }),
    )?;
    let response = read_message(stdout)?.expect("tool response");
    assert_eq!(response["id"], json!(id));
    Ok(response["result"].clone())
}

fn spawn_server(_workspace_root: &Path) -> Result<Child, io::Error> {
    let binary = env!("CARGO_BIN_EXE_rust-lsp-plugin");
    let analyzer = env!("CARGO_BIN_EXE_mock_rust_analyzer");
    Command::new(binary)
        .env("__rust_lsp_plugin_TEST_BIN", analyzer)
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

fn create_temp_workspace(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time went backwards")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "rust-lsp-plugin-{label}-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&path).expect("create temp workspace");
    path
}

fn remove_temp_workspace(path: &Path) {
    let _ = fs::remove_dir_all(path);
}
