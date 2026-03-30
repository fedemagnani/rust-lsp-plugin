//! Runtime primitives for speaking JSON-RPC 2.0 with a stdio-backed LSP server.

pub mod lsp;
pub mod session;
pub mod workspace;

pub use lsp::*;
pub use lsp_server::{Notification, Request, RequestId, Response, ResponseError};
pub use session::{
    Session, SessionBuilder, SessionError, SessionEvent,
};
pub use workspace::{
    TrackedDocument, WorkspaceLoadingState, WorkspaceReadyState, WorkspaceSession,
    WorkspaceSessionBuilder, WorkspaceSessionError, WorkspaceSessionPhase,
};
