#![allow(missing_docs)]

use rust_lsp_mcp::{JsonRpcId, ServerRequest, Session, SessionBuilder, SessionEvent};
use serde_json::json;
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

fn spawn_session() -> Session {
    let binary = env!("CARGO_BIN_EXE_mock_rust_analyzer");
    SessionBuilder::new(binary).spawn().expect("spawn session")
}

fn spawn_shutdown_failure_session() -> Session {
    let binary = env!("CARGO_BIN_EXE_mock_rust_analyzer");
    SessionBuilder::new(binary)
        .env("MOCK_SHUTDOWN_FAILURE", "1")
        .spawn()
        .expect("spawn failing session")
}

fn spawn_timeout_session(timeout: Duration) -> Session {
    let binary = env!("CARGO_BIN_EXE_mock_rust_analyzer");
    SessionBuilder::new(binary)
        .request_timeout(timeout)
        .spawn()
        .expect("spawn timed session")
}

fn spawn_hung_exit_session(timeout: Duration) -> Session {
    let binary = env!("CARGO_BIN_EXE_mock_rust_analyzer");
    SessionBuilder::new(binary)
        .env("MOCK_HANG_ON_EXIT", "1")
        .shutdown_timeout(timeout)
        .spawn()
        .expect("spawn hanging session")
}

fn recv_event(events: &Receiver<SessionEvent>) -> SessionEvent {
    events
        .recv_timeout(Duration::from_secs(2))
        .expect("expected session event")
}

#[test]
fn session_correlates_requests_and_notifications() {
    let session = spawn_session();
    let events = session.take_event_receiver().expect("take event receiver");

    match recv_event(&events) {
        SessionEvent::Stderr(line) => assert!(line.contains("ready")),
        other => panic!("unexpected first event: {other:?}"),
    }

    let response = session
        .request("ping", json!({"message": "hello"}))
        .expect("ping request");
    assert_eq!(response, json!({"echo": {"message": "hello"}}));

    session
        .notify(
            "workspace/didChangeConfiguration",
            json!({"settings": {"checkOnSave": true}}),
        )
        .expect("send notification");

    match recv_event(&events) {
        SessionEvent::Progress { token, value } => {
            assert_eq!(token, json!("mock-progress"));
            assert_eq!(
                value["message"],
                json!("saw:workspace/didChangeConfiguration")
            );
        }
        other => panic!("unexpected event after notification: {other:?}"),
    }

    match recv_event(&events) {
        SessionEvent::Notification { method, params } => {
            assert_eq!(method, "$/progress");
            assert_eq!(
                params.expect("progress params")["token"],
                json!("mock-progress")
            );
        }
        other => panic!("unexpected notification event: {other:?}"),
    }

    let state = session
        .request("state", json!(null))
        .expect("state request");
    assert_eq!(
        state["notifications"],
        json!(["workspace/didChangeConfiguration"])
    );
}

#[test]
fn session_exposes_server_requests_and_supports_cancellation() {
    let session = spawn_session();
    let events = session.take_event_receiver().expect("take event receiver");
    let _ = recv_event(&events);

    let response = session
        .request("server_request", json!({}))
        .expect("server request trigger");
    assert_eq!(response, json!({"status": "request-sent"}));

    match recv_event(&events) {
        SessionEvent::ServerRequest(ServerRequest { id, method, params }) => {
            assert_eq!(id, JsonRpcId::String("config-1".into()));
            assert_eq!(method, "workspace/configuration");
            assert_eq!(params, Some(json!({"items": []})));
            session
                .respond(id, json!([]))
                .expect("respond to server request");
        }
        other => panic!("unexpected server request event: {other:?}"),
    }

    session
        .cancel_request(JsonRpcId::Integer(42))
        .expect("cancel request");
    let state = session
        .request("state", json!(null))
        .expect("state request");
    assert_eq!(state["cancelled"], json!([42]));
}

#[test]
fn session_shuts_down_cleanly() {
    let session = spawn_session();
    let _ = session.take_event_receiver().expect("take event receiver");

    session.shutdown().expect("shutdown session");
}

#[test]
fn shutdown_failure_keeps_drop_cleanup_enabled() {
    let session = spawn_shutdown_failure_session();
    let _ = session.take_event_receiver().expect("take event receiver");

    let error = session.shutdown().expect_err("shutdown should fail");
    assert!(matches!(error, rust_lsp_mcp::SessionError::Disconnected));
}

#[test]
fn session_request_timeout_fails_instead_of_hanging() {
    let session = spawn_timeout_session(Duration::from_millis(20));
    let _ = session.take_event_receiver().expect("take event receiver");

    let error = session
        .request("slow_ping", json!({"message": "hello"}))
        .expect_err("slow request should time out");
    assert!(matches!(
        error,
        rust_lsp_mcp::SessionError::RequestTimeout { method, .. } if method == "slow_ping"
    ));
}

#[test]
fn session_shutdown_times_out_for_hung_exit() {
    let session = spawn_hung_exit_session(Duration::from_millis(50));
    let _ = session.take_event_receiver().expect("take event receiver");

    let start = Instant::now();
    let error = session.shutdown().expect_err("shutdown should time out");

    assert!(matches!(
        error,
        rust_lsp_mcp::SessionError::ProcessExitTimeout { timeout }
            if timeout == Duration::from_millis(50)
    ));
    assert!(start.elapsed() < Duration::from_secs(1));
}
