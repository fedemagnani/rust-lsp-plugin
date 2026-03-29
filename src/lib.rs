//! Runtime primitives for speaking JSON-RPC 2.0 with a stdio-backed LSP server.

pub mod lsp;
pub mod session;
pub mod workspace;

pub use lsp::*;
pub use session::{
    JsonRpcId, ResponseError, ServerRequest, Session, SessionBuilder, SessionError, SessionEvent,
};
pub use workspace::{
    TrackedDocument, WatchedFileChange, WatchedFileChangeKind, WorkspaceLoadingState,
    WorkspaceReadyState, WorkspaceSession, WorkspaceSessionBuilder, WorkspaceSessionError,
    WorkspaceSessionPhase,
};
