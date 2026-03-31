//! MCP server runtime scaffolding built on `rmcp`.

mod error;
mod schema;

use crate::{WorkspaceSession, WorkspaceSessionBuilder, WorkspaceSessionError, WorkspaceSessionPhase};
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::model::{ProtocolVersion, ServerCapabilities, ServerInfo};
use rmcp::transport::stdio;
use rmcp::{ServiceExt, ServerHandler};
use rmcp_macros::{tool_handler, tool_router};
use std::collections::BTreeMap;
use std::error::Error;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;

pub use error::{ServerError, ServerErrorKind};
pub use schema::*;

/// Fallible result used by the MCP server runtime.
pub type ServerResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

const DEFAULT_WORKSPACE_READY_TIMEOUT: Duration = Duration::from_secs(1);

struct WorkspaceEntry {
    session: Mutex<Option<WorkspaceSession>>,
    spawn_count: AtomicUsize,
}

impl Default for WorkspaceEntry {
    fn default() -> Self {
        Self {
            session: Mutex::new(None),
            spawn_count: AtomicUsize::new(0),
        }
    }
}

impl std::fmt::Debug for WorkspaceEntry {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let session_present = self
            .session
            .lock()
            .map(|slot| slot.is_some())
            .unwrap_or(false);
        f.debug_struct("WorkspaceEntry")
            .field("session_present", &session_present)
            .field("spawn_count", &self.spawn_count.load(Ordering::SeqCst))
            .finish()
    }
}

/// Configuration used to create per-workspace rust-analyzer sessions on demand.
#[derive(Debug, Clone)]
pub struct WorkspaceSessionConfig {
    program: OsString,
    args: Vec<OsString>,
    envs: Vec<(OsString, OsString)>,
    request_timeout: Option<Duration>,
    ready_timeout: Duration,
}

impl WorkspaceSessionConfig {
    /// Creates a new workspace session configuration for the given program.
    pub fn new(program: impl AsRef<OsStr>) -> Self {
        Self {
            program: program.as_ref().to_os_string(),
            args: Vec::new(),
            envs: Vec::new(),
            request_timeout: None,
            ready_timeout: DEFAULT_WORKSPACE_READY_TIMEOUT,
        }
    }

    /// Adds a single command-line argument.
    pub fn arg(mut self, arg: impl AsRef<OsStr>) -> Self {
        self.args.push(arg.as_ref().to_os_string());
        self
    }

    /// Adds multiple command-line arguments.
    pub fn args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<OsStr>,
    {
        self.args
            .extend(args.into_iter().map(|arg| arg.as_ref().to_os_string()));
        self
    }

    /// Adds an environment variable for spawned workspace sessions.
    pub fn env(mut self, key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> Self {
        self.envs
            .push((key.as_ref().to_os_string(), value.as_ref().to_os_string()));
        self
    }

    /// Sets the request timeout used by created sessions.
    pub fn request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = Some(timeout);
        self
    }

    /// Sets the ready timeout used during initialization.
    pub fn ready_timeout(mut self, timeout: Duration) -> Self {
        self.ready_timeout = timeout;
        self
    }

    fn spawn_initialized(
        &self,
        workspace_root: &Path,
    ) -> Result<WorkspaceSession, WorkspaceSessionError> {
        let mut builder = WorkspaceSessionBuilder::new(&self.program, workspace_root)
            .args(&self.args)
            .ready_timeout(self.ready_timeout);

        if let Some(request_timeout) = self.request_timeout {
            builder = builder.request_timeout(request_timeout);
        }

        for (key, value) in &self.envs {
            builder = builder.env(key, value);
        }

        let mut session = builder.spawn()?;
        session.initialize()?;
        Ok(session)
    }
}

/// Shared server-owned runtime state that is intentionally kept outside the LSP client layer.
#[derive(Debug, Default)]
pub struct ServerState {
    session_config: RwLock<Option<WorkspaceSessionConfig>>,
    workspaces: RwLock<BTreeMap<PathBuf, Arc<WorkspaceEntry>>>,
}

impl ServerState {
    /// Sets the session configuration used for on-demand workspace creation.
    pub fn set_workspace_session_config(&self, config: WorkspaceSessionConfig) {
        *self
            .session_config
            .write()
            .expect("workspace session config poisoned") = Some(config);
    }

    /// Returns the configured session config, if any.
    pub fn workspace_session_config(&self) -> Option<WorkspaceSessionConfig> {
        self.session_config
            .read()
            .expect("workspace session config poisoned")
            .clone()
    }

    /// Registers a workspace root for future tool routing.
    pub fn insert_workspace_root(&self, root: impl AsRef<Path>) -> Result<bool, ServerError> {
        let root = normalize_registered_workspace_root(root.as_ref())?;
        let mut workspaces = self.workspaces.write().expect("workspace registry poisoned");
        Ok(match workspaces.entry(root) {
            std::collections::btree_map::Entry::Vacant(entry) => {
                entry.insert(Arc::new(WorkspaceEntry::default()));
                true
            }
            std::collections::btree_map::Entry::Occupied(_) => false,
        })
    }

    /// Returns the currently tracked workspace roots.
    pub fn workspace_roots(&self) -> Vec<PathBuf> {
        self.workspaces
            .read()
            .expect("workspace registry poisoned")
            .iter()
            .map(|(root, _)| root.clone())
            .collect()
    }

    /// Returns how many times a root's session has been spawned.
    pub fn session_spawn_count(&self, root: impl AsRef<Path>) -> Result<usize, ServerError> {
        let (root, entry) = self.resolve_workspace_entry(root.as_ref())?;
        let _ = root;
        Ok(entry.spawn_count.load(Ordering::SeqCst))
    }

    /// Routes work to the correct workspace session, creating and initializing it on demand.
    pub fn with_workspace_session<T, F>(
        &self,
        root: impl AsRef<Path>,
        operation: &'static str,
        f: F,
    ) -> Result<T, ServerError>
    where
        F: FnOnce(&mut WorkspaceSession) -> Result<T, WorkspaceSessionError>,
    {
        let (root, entry) = self.resolve_workspace_entry(root.as_ref())?;
        let config = self
            .workspace_session_config()
            .ok_or_else(|| ServerError::internal("workspace session config is not set"))?;

        let mut session_slot = entry.session.lock().expect("workspace session poisoned");
        let must_spawn = match session_slot.as_ref() {
            None => true,
            Some(session) => matches!(
                session.phase(),
                WorkspaceSessionPhase::Failed | WorkspaceSessionPhase::Shutdown
            ),
        };

        if must_spawn {
            *session_slot = Some(
                config
                    .spawn_initialized(&root)
                    .map_err(ServerError::from)?
            );
            entry.spawn_count.fetch_add(1, Ordering::SeqCst);
        }

        let session = session_slot
            .as_mut()
            .expect("workspace session initialized before routing");
        f(session).map_err(|error| ServerError::from(error).with_operation(operation))
    }

    fn resolve_workspace_entry(
        &self,
        requested_root: &Path,
    ) -> Result<(PathBuf, Arc<WorkspaceEntry>), ServerError> {
        let root = normalize_requested_workspace_root(requested_root)?;
        let workspaces = self.workspaces.read().expect("workspace registry poisoned");
        let entry = workspaces
            .get(&root)
            .cloned()
            .ok_or_else(|| ServerError::workspace_not_found(&root))?;
        Ok((root, entry))
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

fn normalize_registered_workspace_root(root: &Path) -> Result<PathBuf, ServerError> {
    if !root.is_absolute() {
        return Err(ServerError::invalid_input(
            "workspace root must be an absolute path",
        ));
    }

    let normalized = std::fs::canonicalize(root).map_err(|_| {
        ServerError::invalid_input(format!(
            "workspace root is not available on disk: {}",
            root.display()
        ))
    })?;

    if !normalized.is_dir() {
        return Err(ServerError::invalid_input(format!(
            "workspace root must point to a directory: {}",
            normalized.display()
        )));
    }

    Ok(normalized)
}

fn normalize_requested_workspace_root(root: &Path) -> Result<PathBuf, ServerError> {
    if !root.is_absolute() {
        return Err(ServerError::invalid_input(
            "workspace root must be an absolute path",
        ));
    }

    std::fs::canonicalize(root).map_err(|_| ServerError::workspace_not_found(root))
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
