#![allow(missing_docs)]

use rust_lsp_mcp::{RustAnalyzerMcpServer, WorkspaceSessionConfig};

#[tokio::main(flavor = "current_thread")]
async fn main() -> rust_lsp_mcp::ServerResult<()> {
    let server = RustAnalyzerMcpServer::new();
    configure_from_env(&server)?;
    server.serve_stdio().await
}

fn configure_from_env(server: &RustAnalyzerMcpServer) -> rust_lsp_mcp::ServerResult<()> {
    if let Some(program) = std::env::var_os("RUST_LSP_MCP_RUST_ANALYZER_BIN") {
        server
            .state()
            .set_workspace_session_config(WorkspaceSessionConfig::new(program));
    }

    if let Ok(max) = std::env::var("RUST_LSP_MCP_MAX_WORKSPACES") {
        let max: usize = max.parse().map_err(|_| -> Box<dyn std::error::Error + Send + Sync> {
            format!("RUST_LSP_MCP_MAX_WORKSPACES must be a positive integer, got: {max}").into()
        })?;
        server.state().set_max_workspaces(max);
    }

    Ok(())
}
