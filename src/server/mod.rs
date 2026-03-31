//! MCP server runtime scaffolding built on `rmcp`.

mod error;
mod schema;

use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::model::{ProtocolVersion, ServerCapabilities, ServerInfo};
use rmcp::transport::stdio;
use rmcp::{ServiceExt, ServerHandler};
use rmcp_macros::{tool_handler, tool_router};
use std::collections::BTreeSet;
use std::error::Error;
use std::path::{Path, PathBuf};
use std::sync::{Arc, RwLock};

pub use error::{ServerError, ServerErrorKind};
pub use schema::*;

/// Fallible result used by the MCP server runtime.
pub type ServerResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

/// Shared server-owned runtime state that is intentionally kept outside the LSP client layer.
#[derive(Debug, Default)]
pub struct ServerState {
    workspace_roots: RwLock<BTreeSet<PathBuf>>,
}

impl ServerState {
    /// Registers a workspace root for future tool routing.
    pub fn insert_workspace_root(&self, root: impl AsRef<Path>) -> bool {
        self.workspace_roots
            .write()
            .expect("workspace roots poisoned")
            .insert(root.as_ref().to_path_buf())
    }

    /// Returns the currently tracked workspace roots.
    pub fn workspace_roots(&self) -> Vec<PathBuf> {
        self.workspace_roots
            .read()
            .expect("workspace roots poisoned")
            .iter()
            .cloned()
            .collect()
    }
}

/// Minimal `rmcp` server shell that owns MCP runtime state separately from the LSP client code.
#[derive(Clone, Debug)]
pub struct RustAnalyzerMcpServer {
    state: Arc<ServerState>,
    tool_router: ToolRouter<Self>,
}

impl Default for RustAnalyzerMcpServer {
    fn default() -> Self {
        Self::new()
    }
}

impl RustAnalyzerMcpServer {
    /// Creates a server with empty runtime state and an extendable tool router.
    pub fn new() -> Self {
        Self {
            state: Arc::new(ServerState::default()),
            tool_router: Self::tool_router(),
        }
    }

    /// Returns the shared runtime state owned by the server layer.
    pub fn state(&self) -> Arc<ServerState> {
        Arc::clone(&self.state)
    }

    /// Starts serving MCP traffic over stdio and waits until the transport closes.
    pub async fn serve_stdio(self) -> ServerResult<()> {
        let running_service = self
            .serve(stdio())
            .await
            .map_err(|error| -> Box<dyn Error + Send + Sync> { Box::new(error) })?;
        running_service
            .waiting()
            .await
            .map_err(|error| -> Box<dyn Error + Send + Sync> { Box::new(error) })?;
        Ok(())
    }
}

#[tool_router]
impl RustAnalyzerMcpServer {}

#[tool_handler]
impl ServerHandler for RustAnalyzerMcpServer {
    fn get_info(&self) -> ServerInfo {
        ServerInfo {
            protocol_version: ProtocolVersion::default(),
            capabilities: ServerCapabilities::builder().enable_tools().build(),
            instructions: Some(
                "Rust analyzer MCP runtime bootstrap. Tool coverage is added by follow-up issues."
                    .into(),
            ),
            server_info: rmcp::model::Implementation::from_build_env(),
            ..Default::default()
        }
    }
}
