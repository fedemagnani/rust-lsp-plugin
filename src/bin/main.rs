#![allow(missing_docs)]

use rust_lsp_plugin::mcp_server::{RustAnalyzerMcpServer, ServerResult, WorkspaceSessionConfig};

#[tokio::main(flavor = "current_thread")]
async fn main() -> ServerResult<()> {
    let server = RustAnalyzerMcpServer::new();

    let program = resolve_rust_analyzer();

    server
        .state()
        .set_workspace_session_config(WorkspaceSessionConfig::new(program));

    server.serve_stdio().await
}

/// Resolves the rust-analyzer binary.
///
/// In normal use, the server expects `rust-analyzer` to be on PATH. Tests can
/// override this by setting `__RUST_LSP_PLUGIN_TEST_BIN` to a mock binary path.
fn resolve_rust_analyzer() -> std::ffi::OsString {
    if let Some(test_bin) = std::env::var_os("__RUST_LSP_PLUGIN_TEST_BIN") {
        return test_bin;
    }

    if which("rust-analyzer").is_none() {
        eprintln!(
            "error: `rust-analyzer` not found in PATH. Install it with:\n\n  \
             rustup component add rust-analyzer\n"
        );
        std::process::exit(1);
    }

    "rust-analyzer".into()
}

fn which(binary: &str) -> Option<std::path::PathBuf> {
    std::env::var_os("PATH").and_then(|paths| {
        std::env::split_paths(&paths).find_map(|dir| {
            let candidate = dir.join(binary);
            candidate.is_file().then_some(candidate)
        })
    })
}
