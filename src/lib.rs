//! Runtime primitives for speaking JSON-RPC 2.0 with a stdio-backed LSP server.

pub mod session;

pub use session::{
    JsonRpcId, ResponseError, ServerRequest, Session, SessionBuilder, SessionError, SessionEvent,
};
