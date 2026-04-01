//! Runtime primitives for speaking JSON-RPC 2.0 with a stdio-backed LSP server.

pub mod rust_analyzer;
pub mod session;
pub mod workspace;

pub use rust_analyzer::*;
pub use session::{Session, SessionBuilder, SessionError, SessionEvent};
pub use workspace::{
    TrackedDocument, WorkspaceLoadingState, WorkspaceReadyState, WorkspaceSession,
    WorkspaceSessionBuilder, WorkspaceSessionError, WorkspaceSessionPhase,
};
