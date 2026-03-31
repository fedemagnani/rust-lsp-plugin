#![allow(missing_docs)]

use rust_lsp_mcp::{ServerErrorKind, RustAnalyzerMcpServer, WorkspaceSessionConfig, WorkspaceSessionPhase};
use serde_json::json;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

#[test]
fn workspace_registry_reuses_existing_session_for_the_same_root() -> Result<(), Box<dyn Error>> {
    let server = configured_server();
    let workspace_root = create_temp_workspace("registry-reuse");
    server.state().insert_workspace_root(&workspace_root)?;

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
    assert_eq!(server.state().session_spawn_count(&workspace_root)?, 1);

    remove_temp_workspace(&workspace_root);
    Ok(())
}

#[test]
fn workspace_registry_routes_requests_by_workspace_root() -> Result<(), Box<dyn Error>> {
    let server = configured_server();
    let workspace_one = create_temp_workspace("registry-route-one");
    let workspace_two = create_temp_workspace("registry-route-two");
    server.state().insert_workspace_root(&workspace_one)?;
    server.state().insert_workspace_root(&workspace_two)?;

    let first_state = server
        .state()
        .with_workspace_session(&workspace_one, "request", |session| {
            session.request("state", json!(null))
        })?;
    let second_state = server
        .state()
        .with_workspace_session(&workspace_two, "request", |session| {
            session.request("state", json!(null))
        })?;

    assert_ne!(
        first_state["initialize_params"]["rootPath"],
        second_state["initialize_params"]["rootPath"]
    );
    assert_eq!(server.state().session_spawn_count(&workspace_one)?, 1);
    assert_eq!(server.state().session_spawn_count(&workspace_two)?, 1);

    remove_temp_workspace(&workspace_one);
    remove_temp_workspace(&workspace_two);
    Ok(())
}

#[test]
fn workspace_registry_returns_structured_errors_for_invalid_or_unknown_roots() -> Result<(), Box<dyn Error>> {
    let server = configured_server();
    let registered_root = create_temp_workspace("registry-known");
    let unknown_root = create_temp_workspace("registry-unknown");
    server.state().insert_workspace_root(&registered_root)?;

    let relative_error = server
        .state()
        .with_workspace_session(PathBuf::from("relative/root"), "request", |_session| -> Result<(), rust_lsp_mcp::WorkspaceSessionError> {
            unreachable!("relative roots should fail before routing")
        })
        .expect_err("relative root should be rejected");
    assert_eq!(relative_error.kind, ServerErrorKind::InvalidInput);

    let unknown_error = server
        .state()
        .with_workspace_session(&unknown_root, "request", |_session| -> Result<(), rust_lsp_mcp::WorkspaceSessionError> {
            unreachable!("unknown roots should fail before routing")
        })
        .expect_err("unknown root should be rejected");
    assert_eq!(unknown_error.kind, ServerErrorKind::WorkspaceNotFound);

    remove_temp_workspace(&registered_root);
    remove_temp_workspace(&unknown_root);
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
