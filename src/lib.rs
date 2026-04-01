//! MCP server backed by rust-analyzer, exposing LSP-powered code intelligence tools.
//!
//! - [`lsp_client`] — JSON-RPC transport and workspace session management for stdio-backed LSP servers.
//! - [`mcp_server`] — MCP server runtime that wraps the LSP client and exposes tools to callers.

pub mod lsp_client;
pub mod mcp_server;
