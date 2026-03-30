#![allow(missing_docs)]

use rust_lsp_mcp::RustAnalyzerMcpServer;

#[tokio::main(flavor = "current_thread")]
async fn main() -> rust_lsp_mcp::ServerResult<()> {
    RustAnalyzerMcpServer::new().serve_stdio().await
}
