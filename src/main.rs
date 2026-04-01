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

    Ok(())
}
