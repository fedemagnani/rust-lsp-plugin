//! Runtime primitives for speaking JSON-RPC 2.0 with a stdio-backed LSP server.

pub mod lsp_client;
pub mod server;

pub use lsp_client::*;
pub use server::*;
