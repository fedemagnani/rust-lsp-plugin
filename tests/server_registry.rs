#![allow(missing_docs)]

use rust_lsp_mcp::lsp_client::WorkspaceSessionPhase;
use rust_lsp_mcp::mcp_server::{RustAnalyzerMcpServer, ServerErrorKind, WorkspaceSessionConfig};
use serde_json::json;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[test]
fn workspace_session_reuses_existing_session_for_the_same_root() -> Result<(), Box<dyn Error>> {
    let server = configured_server();
    let workspace_root = create_temp_workspace("session-reuse");

    let first_state = server
        .state()
        .with_workspace_session(&workspace_root, "request", |session| {
            assert_eq!(session.phase(), WorkspaceSessionPhase::Ready);
            session.request("state", json!(null))
        })?;
    let second_state = server
        .state()
        .with_workspace_session(&workspace_root, "request", |session| {
            assert_eq!(session.phase(), WorkspaceSessionPhase::Ready);
            session.request("state", json!(null))
        })?;

    assert_eq!(
        first_state["initialize_params"]["rootPath"],
        second_state["initialize_params"]["rootPath"]
    );

    remove_temp_workspace(&workspace_root);
    Ok(())
}

#[test]
fn workspace_session_replaces_when_root_changes() -> Result<(), Box<dyn Error>> {
    let server = configured_server();
    let workspace_one = create_temp_workspace("session-replace-one");
    let workspace_two = create_temp_workspace("session-replace-two");

    server
        .state()
        .with_workspace_session(&workspace_one, "request", |session| {
            session.request("state", json!(null))
        })?;
    assert_eq!(
        server.state().active_workspace_root().as_deref(),
        Some(std::fs::canonicalize(&workspace_one)?.as_path())
    );

    server
        .state()
        .with_workspace_session(&workspace_two, "request", |session| {
            session.request("state", json!(null))
        })?;
    assert_eq!(
        server.state().active_workspace_root().as_deref(),
        Some(std::fs::canonicalize(&workspace_two)?.as_path())
    );

    remove_temp_workspace(&workspace_one);
    remove_temp_workspace(&workspace_two);
    Ok(())
}

#[test]
fn workspace_session_returns_structured_errors_for_invalid_roots() -> Result<(), Box<dyn Error>> {
    let server = configured_server();

    let relative_error = server
        .state()
        .with_workspace_session(PathBuf::from("relative/root"), "request", |_session| -> Result<(), rust_lsp_mcp::lsp_client::WorkspaceSessionError> {
            unreachable!("relative roots should fail before routing")
        })
        .expect_err("relative root should be rejected");
    assert_eq!(relative_error.kind, ServerErrorKind::InvalidInput);

    let nonexistent = PathBuf::from("/tmp/rust-lsp-mcp-nonexistent-root-that-does-not-exist");
    let nonexistent_error = server
        .state()
        .with_workspace_session(&nonexistent, "request", |_session| -> Result<(), rust_lsp_mcp::lsp_client::WorkspaceSessionError> {
            unreachable!("nonexistent roots should fail before routing")
        })
        .expect_err("nonexistent root should be rejected");
    assert_eq!(nonexistent_error.kind, ServerErrorKind::InvalidInput);

    Ok(())
}

fn configured_server() -> RustAnalyzerMcpServer {
    let server = RustAnalyzerMcpServer::new();
    let binary = env!("CARGO_BIN_EXE_mock_rust_analyzer");
    server.state().set_workspace_session_config(
        WorkspaceSessionConfig::new(binary).ready_timeout(Duration::from_millis(200)),
    );
    server
}

fn create_temp_workspace(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time went backwards")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "rust-lsp-mcp-{label}-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&path).expect("create temp workspace");
    path
}

fn remove_temp_workspace(path: &Path) {
    let _ = fs::remove_dir_all(path);
}
