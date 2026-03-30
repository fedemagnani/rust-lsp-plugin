//! Stable MCP-facing error taxonomy for rust-analyzer-backed tools.

use crate::{SessionError, WorkspaceSessionError};
use rmcp::{ErrorData, model::ErrorCode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::borrow::Cow;
use std::path::{Path, PathBuf};
use std::time::Duration;

const REQUEST_CANCELLED_CODE: ErrorCode = ErrorCode(-32800);
const REQUEST_TIMEOUT_CODE: ErrorCode = ErrorCode(-32001);
const CAPABILITY_UNSUPPORTED_CODE: ErrorCode = ErrorCode(-32004);

/// Stable error categories surfaced by the MCP server layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ServerErrorKind {
    /// The caller supplied invalid or incomplete input.
    InvalidInput,
    /// The requested workspace root is unknown to the server.
    WorkspaceNotFound,
    /// The requested document is missing or not synchronized.
    DocumentNotAvailable,
    /// The underlying rust-analyzer workspace is not yet ready.
    NotReady,
    /// The request was cancelled before completion.
    Cancelled,
    /// The request exceeded its configured deadline.
    Timeout,
    /// The downstream protocol returned an invalid or unsupported payload.
    Protocol,
    /// The requested operation is unsupported by the negotiated capabilities.
    CapabilityUnsupported,
    /// An internal server failure occurred.
    Internal,
}

impl ServerErrorKind {
    fn error_code(self) -> ErrorCode {
        match self {
            Self::InvalidInput => ErrorCode::INVALID_PARAMS,
            Self::WorkspaceNotFound | Self::DocumentNotAvailable => ErrorCode::RESOURCE_NOT_FOUND,
            Self::NotReady => ErrorCode::INVALID_REQUEST,
            Self::Cancelled => REQUEST_CANCELLED_CODE,
            Self::Timeout => REQUEST_TIMEOUT_CODE,
            Self::Protocol => ErrorCode::INTERNAL_ERROR,
            Self::CapabilityUnsupported => CAPABILITY_UNSUPPORTED_CODE,
            Self::Internal => ErrorCode::INTERNAL_ERROR,
        }
    }
}

/// Structured MCP-facing error value used before conversion into rmcp transport errors.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct ServerError {
    /// Stable error category.
    pub kind: ServerErrorKind,
    /// Short human-readable summary.
    pub message: String,
    /// Operation name that produced the failure, when known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub operation: Option<String>,
    /// Whether retrying the operation could plausibly succeed.
    pub retriable: bool,
    /// Additional structured context for callers.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
}

impl ServerError {
    /// Creates a new server error.
    pub fn new(kind: ServerErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            operation: None,
            retriable: false,
            details: None,
        }
    }

    /// Marks the error with the operation that failed.
    pub fn with_operation(mut self, operation: impl Into<String>) -> Self {
        self.operation = Some(operation.into());
        self
    }

    /// Marks the error as plausibly retriable.
    pub fn retriable(mut self, retriable: bool) -> Self {
        self.retriable = retriable;
        self
    }

    /// Adds structured details for downstream callers.
    pub fn with_details(mut self, details: Value) -> Self {
        self.details = Some(details);
        self
    }

    /// Convenience constructor for invalid input errors.
    pub fn invalid_input(message: impl Into<String>) -> Self {
        Self::new(ServerErrorKind::InvalidInput, message)
    }

    /// Convenience constructor for unknown workspace roots.
    pub fn workspace_not_found(root: impl AsRef<Path>) -> Self {
        Self::new(
            ServerErrorKind::WorkspaceNotFound,
            format!("workspace root is not registered: {}", root.as_ref().display()),
        )
        .with_details(json!({ "workspace_root": root.as_ref() }))
    }

    /// Convenience constructor for missing documents.
    pub fn document_not_available(path: impl AsRef<Path>) -> Self {
        Self::new(
            ServerErrorKind::DocumentNotAvailable,
            format!("document is not available: {}", path.as_ref().display()),
        )
        .with_details(json!({ "document_path": path.as_ref() }))
    }

    /// Convenience constructor for workspace readiness failures.
    pub fn not_ready(message: impl Into<String>) -> Self {
        Self::new(ServerErrorKind::NotReady, message).retriable(true)
    }

    /// Convenience constructor for cancelled work.
    pub fn cancelled(message: impl Into<String>) -> Self {
        Self::new(ServerErrorKind::Cancelled, message).retriable(true)
    }

    /// Convenience constructor for timed out work.
    pub fn timeout(operation: impl Into<String>, timeout: Duration) -> Self {
        Self::new(
            ServerErrorKind::Timeout,
            format!("operation timed out after {:?}", timeout),
        )
        .with_operation(operation)
        .retriable(true)
        .with_details(json!({ "timeout_ms": timeout.as_millis() }))
    }

    /// Convenience constructor for downstream protocol failures.
    pub fn protocol(message: impl Into<String>) -> Self {
        Self::new(ServerErrorKind::Protocol, message)
    }

    /// Convenience constructor for unsupported capability failures.
    pub fn capability_unsupported(message: impl Into<String>) -> Self {
        Self::new(ServerErrorKind::CapabilityUnsupported, message)
    }

    /// Convenience constructor for internal server failures.
    pub fn internal(message: impl Into<String>) -> Self {
        Self::new(ServerErrorKind::Internal, message)
    }

    /// Converts the stable error into the rmcp transport error shape.
    pub fn to_error_data(&self) -> ErrorData {
        ErrorData::new(
            self.kind.error_code(),
            Cow::Owned(self.message.clone()),
            Some(json!({
                "kind": self.kind,
                "operation": self.operation,
                "retriable": self.retriable,
                "details": self.details,
            })),
        )
    }
}

impl From<ServerError> for ErrorData {
    fn from(value: ServerError) -> Self {
        value.to_error_data()
    }
}

impl From<SessionError> for ServerError {
    fn from(value: SessionError) -> Self {
        match value {
            SessionError::RequestTimeout { method, timeout } => Self::timeout(method, timeout),
            SessionError::Protocol(message) => Self::protocol(message),
            SessionError::Disconnected => Self::cancelled("rust-analyzer session disconnected"),
            SessionError::ServerError(error) => Self::protocol(format!(
                "rust-analyzer request failed: {}",
                error.message
            )),
            SessionError::ProcessExitTimeout { timeout } => Self::timeout("shutdown", timeout),
            SessionError::Spawn(error) | SessionError::Io(error) => Self::internal(error.to_string()),
            SessionError::MissingPipe(pipe) => {
                Self::internal(format!("child process missing {pipe} pipe"))
            }
            SessionError::Json(error) => Self::protocol(error.to_string()),
        }
    }
}

impl From<WorkspaceSessionError> for ServerError {
    fn from(value: WorkspaceSessionError) -> Self {
        match value {
            WorkspaceSessionError::Session(error) => error.into(),
            WorkspaceSessionError::InvalidPhase { operation, phase } => Self::not_ready(format!(
                "operation {operation} is invalid while workspace session is in phase {phase:?}"
            ))
            .with_operation(operation),
            WorkspaceSessionError::MissingEventReceiver => {
                Self::internal("workspace session is missing its event receiver")
            }
            WorkspaceSessionError::DocumentNotOpen { path }
            | WorkspaceSessionError::DocumentAlreadyOpen { path } => {
                Self::document_not_available(path)
            }
            WorkspaceSessionError::NonMonotonicDocumentVersion {
                path,
                current_version,
                new_version,
            } => Self::invalid_input(format!(
                "document version must increase: current={current_version}, new={new_version}"
            ))
            .with_details(json!({
                "document_path": path,
                "current_version": current_version,
                "new_version": new_version,
            })),
            WorkspaceSessionError::InvalidResponse { method, source } => {
                Self::protocol(format!("invalid response for {method}: {source}"))
                    .with_operation(method)
            }
        }
    }
}

const _: () = {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<ServerError>();
};
