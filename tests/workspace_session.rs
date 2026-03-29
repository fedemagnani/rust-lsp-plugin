#![allow(missing_docs)]

use rust_lsp_mcp::{
    SessionEvent, WorkspaceLoadingState, WorkspaceSessionBuilder, WorkspaceSessionError,
    WorkspaceSessionPhase,
};
use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;

fn workspace_root() -> PathBuf {
    std::env::current_dir().expect("workspace root")
}

fn spawn_workspace_session() -> rust_lsp_mcp::WorkspaceSession {
    let binary = env!("CARGO_BIN_EXE_mock_rust_analyzer");
    WorkspaceSessionBuilder::new(binary, workspace_root())
        .ready_timeout(Duration::from_millis(200))
        .spawn()
        .expect("spawn workspace session")
}

fn spawn_failing_initialized_workspace_session() -> rust_lsp_mcp::WorkspaceSession {
    let binary = env!("CARGO_BIN_EXE_mock_rust_analyzer");
    WorkspaceSessionBuilder::new(binary, workspace_root())
        .env("MOCK_INITIALIZED_FAILURE", "1")
        .ready_timeout(Duration::from_millis(200))
        .spawn()
        .expect("spawn workspace session")
}

fn spawn_workspace_session_with_extra_startup_progress() -> rust_lsp_mcp::WorkspaceSession {
    let binary = env!("CARGO_BIN_EXE_mock_rust_analyzer");
    WorkspaceSessionBuilder::new(binary, workspace_root())
        .env("MOCK_EXTRA_STARTUP_PROGRESS", "1")
        .ready_timeout(Duration::from_millis(200))
        .spawn()
        .expect("spawn workspace session")
}

#[test]
fn workspace_session_initializes_and_reaches_ready_state() {
    let mut session = spawn_workspace_session();

    assert_eq!(session.phase(), WorkspaceSessionPhase::PreInitialize);

    let ready = session.initialize().expect("initialize workspace").clone();

    assert_eq!(session.phase(), WorkspaceSessionPhase::Ready);
    assert_eq!(ready.server_capabilities["hoverProvider"], json!(true));
    assert_eq!(
        ready.server_capabilities["positionEncoding"],
        json!("utf-8")
    );
    assert_eq!(
        ready.server_info.as_ref().expect("server info")["name"],
        json!("mock-rust-analyzer")
    );
    assert!(ready.configuration_requested);
    assert_eq!(ready.loading_state, WorkspaceLoadingState::Ready);
    assert_eq!(session.loading_state(), &WorkspaceLoadingState::Ready);

    let state = session
        .request("state", json!(null))
        .expect("state request");
    assert_eq!(state["initialized_received"], json!(true));
    assert_eq!(
        state["initialize_params"]["capabilities"]["general"]["positionEncodings"],
        json!(["utf-8", "utf-16", "utf-32"])
    );
    assert_eq!(
        state["initialize_params"]["capabilities"]["experimental"]["serverStatusNotification"],
        json!(true)
    );
    assert_eq!(
        state["initialize_params"]["initializationOptions"]["cargo"]["buildScripts"]["enable"],
        json!(true)
    );
    assert_eq!(
        state["config_response"],
        json!([
            {
                "cargo": {
                    "autoreload": true,
                    "buildScripts": { "enable": true }
                },
                "checkOnSave": true,
                "files": { "watcher": "client" },
                "procMacro": { "enable": true }
            },
            { "enable": true }
        ])
    );
}

#[test]
fn workspace_session_buffers_non_handshake_events() {
    let mut session = spawn_workspace_session();
    session.initialize().expect("initialize workspace");

    match session
        .next_event(Duration::from_millis(20))
        .expect("read next event")
    {
        Some(SessionEvent::Stderr(line)) => assert!(line.contains("ready")),
        other => panic!("unexpected buffered event: {other:?}"),
    }

    session
        .notify(
            "workspace/didChangeConfiguration",
            json!({"settings": {"checkOnSave": true}}),
        )
        .expect("configuration notification");

    match session
        .next_event(Duration::from_secs(1))
        .expect("read next event")
    {
        Some(SessionEvent::Progress { token, value }) => {
            assert_eq!(token, json!("mock-progress"));
            assert_eq!(
                value["message"],
                json!("saw:workspace/didChangeConfiguration")
            );
        }
        other => panic!("unexpected event: {other:?}"),
    }
}

#[test]
fn workspace_session_ignores_non_workspace_progress_tokens() {
    let mut session = spawn_workspace_session_with_extra_startup_progress();

    let ready = session.initialize().expect("initialize workspace").clone();

    assert_eq!(ready.loading_state, WorkspaceLoadingState::Ready);
    assert_eq!(session.loading_state(), &WorkspaceLoadingState::Ready);
}

#[test]
fn workspace_session_rejects_requests_before_initialize() {
    let session = spawn_workspace_session();

    let error = session
        .request("state", json!(null))
        .expect_err("request should fail before initialize");
    assert!(matches!(
        error,
        WorkspaceSessionError::InvalidPhase {
            operation: "request",
            phase: WorkspaceSessionPhase::PreInitialize,
        }
    ));
}

#[test]
fn workspace_session_enters_failed_phase_when_initialize_fails() {
    let mut session = spawn_failing_initialized_workspace_session();

    let error = session
        .initialize()
        .expect_err("initialize should fail when initialized handling disconnects");
    assert!(matches!(
        error,
        WorkspaceSessionError::Session(rust_lsp_mcp::SessionError::Disconnected)
    ));
    assert_eq!(session.phase(), WorkspaceSessionPhase::Failed);

    let retry_error = session
        .initialize()
        .expect_err("failed session should reject initialize retries");
    assert!(matches!(
        retry_error,
        WorkspaceSessionError::InvalidPhase {
            operation: "initialize",
            phase: WorkspaceSessionPhase::Failed,
        }
    ));
}
