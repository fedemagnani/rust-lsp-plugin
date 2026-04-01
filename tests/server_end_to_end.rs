#![allow(missing_docs)]

use serde_json::{Value, json};
use std::error::Error;
use std::fs;
use std::io::{self, BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

// ---------------------------------------------------------------------------
// Workspace switching: tool calls with a different root replace the session
// ---------------------------------------------------------------------------

#[test]
fn mcp_server_switches_workspace_when_root_changes() -> Result<(), Box<dyn Error>> {
    let workspace_a = create_temp_workspace("e2e-switch-a");
    let workspace_b = create_temp_workspace("e2e-switch-b");
    let file_a = workspace_a.join("src").join("lib.rs");
    let file_b = workspace_b.join("src").join("lib.rs");
    fs::create_dir_all(file_a.parent().unwrap())?;
    fs::create_dir_all(file_b.parent().unwrap())?;
    fs::write(&file_a, "pub fn alpha() -> u32 { 1 }\n")?;
    fs::write(&file_b, "pub fn beta() -> u32 { 2 }\n")?;

    let mut child = spawn_server(&workspace_a)?;
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    initialize_server(&mut stdin, &mut stdout)?;

    let hover_a = call_tool(
        &mut stdin,
        &mut stdout,
        2,
        "hover",
        json!({
            "workspace_root": workspace_a,
            "document_path": file_a,
            "position": { "line": 0, "character": 7 }
        }),
    )?;
    assert!(
        hover_a.get("structuredContent").is_some(),
        "hover on workspace A returned data"
    );

    let hover_b = call_tool(
        &mut stdin,
        &mut stdout,
        3,
        "hover",
        json!({
            "workspace_root": workspace_b,
            "document_path": file_b,
            "position": { "line": 0, "character": 7 }
        }),
    )?;
    assert!(
        hover_b.get("structuredContent").is_some(),
        "hover on workspace B returned data"
    );

    let symbols_a = call_tool(
        &mut stdin,
        &mut stdout,
        4,
        "workspace_symbols",
        json!({ "workspace_root": workspace_a, "query": "alpha" }),
    )?;
    assert_eq!(
        symbols_a["structuredContent"]["data"][0]["name"],
        json!("alpha"),
    );

    let symbols_b = call_tool(
        &mut stdin,
        &mut stdout,
        5,
        "workspace_symbols",
        json!({ "workspace_root": workspace_b, "query": "beta" }),
    )?;
    assert_eq!(
        symbols_b["structuredContent"]["data"][0]["name"],
        json!("beta"),
    );

    drop(stdin);
    wait_for_exit(&mut child, Duration::from_secs(2))?;
    remove_temp_workspace(&workspace_a);
    remove_temp_workspace(&workspace_b);
    Ok(())
}

// ---------------------------------------------------------------------------
// Structured failure: unknown workspace root produces a structured MCP error
// ---------------------------------------------------------------------------

#[test]
fn mcp_server_returns_structured_error_for_nonexistent_workspace() -> Result<(), Box<dyn Error>> {
    let workspace = create_temp_workspace("e2e-error-known");
    let nonexistent =
        PathBuf::from("/tmp/rust-lsp-plugin-nonexistent-workspace-that-does-not-exist");
    let file = workspace.join("src").join("lib.rs");
    fs::create_dir_all(file.parent().unwrap())?;
    fs::write(&file, "pub fn x() {}\n")?;

    let mut child = spawn_server(&workspace)?;
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    initialize_server(&mut stdin, &mut stdout)?;

    write_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 2,
            "method": "tools/call",
            "params": {
                "name": "hover",
                "arguments": {
                    "workspace_root": nonexistent,
                    "document_path": nonexistent.join("src/lib.rs"),
                    "position": { "line": 0, "character": 0 }
                }
            }
        }),
    )?;
    let response = read_message(&mut stdout)?.expect("error response");
    assert_eq!(response["id"], json!(2));

    let has_error = response.get("error").is_some()
        || response
            .get("result")
            .and_then(|r| r.get("isError"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
    assert!(
        has_error,
        "expected a structured error for nonexistent workspace, got: {response}"
    );

    drop(stdin);
    wait_for_exit(&mut child, Duration::from_secs(2))?;
    remove_temp_workspace(&workspace);
    Ok(())
}

// ---------------------------------------------------------------------------
// Stateful tool flows: document sync, rename, reload, rebuild through MCP
// ---------------------------------------------------------------------------

#[test]
fn mcp_server_exercises_stateful_tool_flows() -> Result<(), Box<dyn Error>> {
    let workspace = create_temp_workspace("e2e-stateful");
    let file = workspace.join("src").join("lib.rs");
    fs::create_dir_all(file.parent().unwrap())?;
    fs::write(&file, "pub fn answer() -> u32 { 42 }\n")?;

    let mut child = spawn_server(&workspace)?;
    let mut stdin = child.stdin.take().unwrap();
    let mut stdout = BufReader::new(child.stdout.take().unwrap());

    initialize_server(&mut stdin, &mut stdout)?;
    let mut next_id: u64 = 2;

    // -- open_document --
    let open_result = call_tool(
        &mut stdin,
        &mut stdout,
        next_id,
        "open_document",
        json!({
            "workspace_root": workspace,
            "document_path": file,
            "text": "pub fn answer() -> u32 { 42 }\n"
        }),
    )?;
    next_id += 1;
    assert_eq!(
        open_result["structuredContent"]["data"]["version"],
        json!(0)
    );
    assert!(
        open_result["structuredContent"]["data"]["document_path"]
            .as_str()
            .unwrap()
            .ends_with("/src/lib.rs"),
    );

    // -- change_document --
    let change_result = call_tool(
        &mut stdin,
        &mut stdout,
        next_id,
        "change_document",
        json!({
            "workspace_root": workspace,
            "document_path": file,
            "version": 1,
            "text": "pub fn renamed_answer() -> u32 { 42 }\n"
        }),
    )?;
    next_id += 1;
    assert_eq!(
        change_result["structuredContent"]["data"]["version"],
        json!(1)
    );

    // -- rename_symbol --
    let rename_result = call_tool(
        &mut stdin,
        &mut stdout,
        next_id,
        "rename_symbol",
        json!({
            "workspace_root": workspace,
            "document_path": file,
            "position": { "line": 0, "character": 7 },
            "new_name": "better_answer"
        }),
    )?;
    next_id += 1;
    assert_eq!(
        rename_result["structuredContent"]["data"]["new_name"],
        json!("better_answer"),
    );
    let workspace_edit = &rename_result["structuredContent"]["workspace_edit"];
    assert!(
        workspace_edit["changes"]
            .as_array()
            .is_some_and(|c| !c.is_empty()),
        "rename should produce workspace edit changes"
    );

    // -- close_document --
    let close_result = call_tool(
        &mut stdin,
        &mut stdout,
        next_id,
        "close_document",
        json!({
            "workspace_root": workspace,
            "document_path": file,
        }),
    )?;
    next_id += 1;
    assert!(
        close_result["structuredContent"]["data"]["document_path"]
            .as_str()
            .unwrap()
            .ends_with("/src/lib.rs"),
    );

    // -- reload_workspace --
    let reload_result = call_tool(
        &mut stdin,
        &mut stdout,
        next_id,
        "reload_workspace",
        json!({ "workspace_root": workspace }),
    )?;
    next_id += 1;
    assert!(
        reload_result.get("isError").is_none() || reload_result["isError"] == json!(false),
        "reload_workspace should succeed"
    );

    // -- rebuild_proc_macros --
    let rebuild_result = call_tool(
        &mut stdin,
        &mut stdout,
        next_id,
        "rebuild_proc_macros",
        json!({ "workspace_root": workspace }),
    )?;
    let _ = next_id;
    assert!(
        rebuild_result.get("isError").is_none() || rebuild_result["isError"] == json!(false),
        "rebuild_proc_macros should succeed"
    );

    // -- verify tool annotations distinguish stateful from read-only --
    write_message(
        &mut stdin,
        &json!({
            "jsonrpc": "2.0",
            "id": 100,
            "method": "tools/list"
        }),
    )?;
    let tools_response = read_message(&mut stdout)?.expect("tools list");
    let tools = tools_response["result"]["tools"]
        .as_array()
        .expect("tools array");

    let stateful_names = [
        "open_document",
        "change_document",
        "replace_document",
        "close_document",
        "rename_symbol",
        "reload_workspace",
        "rebuild_proc_macros",
    ];
    for name in &stateful_names {
        let tool = tools
            .iter()
            .find(|t| t["name"].as_str() == Some(name))
            .unwrap_or_else(|| panic!("expected tool {name} in listing"));
        assert_eq!(
            tool["annotations"]["readOnlyHint"],
            json!(false),
            "{name} should not be read-only"
        );
    }

    drop(stdin);
    wait_for_exit(&mut child, Duration::from_secs(2))?;
    remove_temp_workspace(&workspace);
    Ok(())
}

// ---------------------------------------------------------------------------
// Shared helpers (same pattern as existing test files)
// ---------------------------------------------------------------------------

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
                    "name": "server-end-to-end-test",
                    "version": "0.0.0"
                }
            }
        }),
    )?;
    let response = read_message(stdout)?.expect("initialize response");
    assert_eq!(response["id"], json!(1));

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
