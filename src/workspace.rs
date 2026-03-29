//! Workspace-scoped rust-analyzer session management built on top of the JSON-RPC transport.

use crate::{Session, SessionBuilder, SessionError, SessionEvent};
use serde::Serialize;
use serde_json::{json, Value};
use std::collections::VecDeque;
use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf, Prefix};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;

const DEFAULT_READY_TIMEOUT: Duration = Duration::from_secs(1);

/// High-level session phase for the LSP initialization lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceSessionPhase {
    /// The transport exists, but `initialize` has not been sent yet.
    PreInitialize,
    /// The client is performing the initialize and initialized handshake.
    Initializing,
    /// The workspace completed its startup handshake and can serve later requests.
    Ready,
    /// Shutdown completed and the session is no longer usable.
    Shutdown,
}

/// Observed workspace loading state normalized from `$/progress`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorkspaceLoadingState {
    /// No workspace loading activity has been observed yet.
    NotStarted,
    /// Workspace loading is still in progress.
    InProgress {
        /// Most recent progress message reported by the server, when present.
        message: Option<String>,
    },
    /// Workspace loading has finished.
    Ready,
}

/// Structured summary produced by a successful `initialize` handshake.
#[derive(Debug, Clone, PartialEq)]
pub struct WorkspaceReadyState {
    /// The raw LSP server capabilities from the initialize response.
    pub server_capabilities: Value,
    /// Optional server information from the initialize response.
    pub server_info: Option<Value>,
    /// Whether the server requested workspace configuration during initialization.
    pub configuration_requested: bool,
    /// Workspace loading progress observed during the handshake.
    pub loading_state: WorkspaceLoadingState,
}

/// Errors produced by the workspace session layer.
#[derive(Debug, thiserror::Error)]
pub enum WorkspaceSessionError {
    /// A lower-level transport or JSON-RPC operation failed.
    #[error(transparent)]
    Session(#[from] SessionError),
    /// The requested operation is invalid for the current session phase.
    #[error("workspace session operation {operation} is invalid in phase {phase:?}")]
    InvalidPhase {
        /// Operation that was attempted.
        operation: &'static str,
        /// Current session phase.
        phase: WorkspaceSessionPhase,
    },
    /// The initialize result did not contain the expected fields.
    #[error("initialize response missing {field}")]
    MissingInitializeField {
        /// Missing response field name.
        field: &'static str,
    },
    /// The event receiver was not available when the workspace session was created.
    #[error("workspace session is missing the event receiver")]
    MissingEventReceiver,
}

/// Builder for a rust-analyzer workspace session with the expected initialization contract.
pub struct WorkspaceSessionBuilder {
    program: OsString,
    args: Vec<OsString>,
    current_dir: Option<PathBuf>,
    envs: Vec<(OsString, OsString)>,
    request_timeout: Option<Duration>,
    ready_timeout: Duration,
    workspace_root: PathBuf,
    client_name: String,
    client_version: Option<String>,
    client_capabilities: Value,
    initialization_options: Value,
    workspace_configuration: Value,
}

impl WorkspaceSessionBuilder {
    /// Creates a builder for a workspace-root-scoped rust-analyzer session.
    pub fn new(program: impl AsRef<OsStr>, workspace_root: impl AsRef<Path>) -> Self {
        Self {
            program: program.as_ref().to_os_string(),
            args: Vec::new(),
            current_dir: None,
            envs: Vec::new(),
            request_timeout: None,
            ready_timeout: DEFAULT_READY_TIMEOUT,
            workspace_root: workspace_root.as_ref().to_path_buf(),
            client_name: env!("CARGO_PKG_NAME").to_owned(),
            client_version: Some(env!("CARGO_PKG_VERSION").to_owned()),
            client_capabilities: default_client_capabilities(),
            initialization_options: default_initialization_options(),
            workspace_configuration: default_workspace_configuration(),
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

    /// Sets the child process working directory.
    pub fn current_dir(mut self, path: impl AsRef<Path>) -> Self {
        self.current_dir = Some(path.as_ref().to_path_buf());
        self
    }

    /// Adds an environment variable for the child process.
    pub fn env(mut self, key: impl AsRef<OsStr>, value: impl AsRef<OsStr>) -> Self {
        self.envs
            .push((key.as_ref().to_os_string(), value.as_ref().to_os_string()));
        self
    }

    /// Sets the default request timeout used by the transport session.
    pub fn request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = Some(timeout);
        self
    }

    /// Sets the maximum time spent waiting for immediate post-`initialized` startup events.
    pub fn ready_timeout(mut self, timeout: Duration) -> Self {
        self.ready_timeout = timeout;
        self
    }

    /// Overrides the rust-analyzer client capabilities sent in the initialize request.
    pub fn client_capabilities(mut self, capabilities: Value) -> Self {
        self.client_capabilities = capabilities;
        self
    }

    /// Overrides rust-analyzer initialization options sent with the initialize request.
    pub fn initialization_options(mut self, options: Value) -> Self {
        self.initialization_options = options;
        self
    }

    /// Overrides the workspace configuration returned to `workspace/configuration`.
    pub fn workspace_configuration(mut self, configuration: Value) -> Self {
        self.workspace_configuration = configuration;
        self
    }

    /// Spawns the child process and creates a pre-initialize workspace session.
    pub fn spawn(self) -> Result<WorkspaceSession, WorkspaceSessionError> {
        let workspace_root = absolutize_path(self.workspace_root)?;
        let workspace_uri = file_uri_from_path(&workspace_root);

        let mut session_builder = SessionBuilder::new(&self.program).args(&self.args);
        if let Some(current_dir) = self.current_dir {
            session_builder = session_builder.current_dir(current_dir);
        }
        if let Some(request_timeout) = self.request_timeout {
            session_builder = session_builder.request_timeout(request_timeout);
        }
        for (key, value) in self.envs {
            session_builder = session_builder.env(key, value);
        }

        let session = session_builder.spawn()?;
        let events = session
            .take_event_receiver()
            .ok_or(WorkspaceSessionError::MissingEventReceiver)?;

        Ok(WorkspaceSession {
            session,
            events,
            buffered_events: VecDeque::new(),
            phase: WorkspaceSessionPhase::PreInitialize,
            workspace_root,
            workspace_uri,
            client_name: self.client_name,
            client_version: self.client_version,
            client_capabilities: self.client_capabilities,
            initialization_options: self.initialization_options,
            workspace_configuration: self.workspace_configuration,
            ready_timeout: self.ready_timeout,
            ready_state: None,
            loading_state: WorkspaceLoadingState::NotStarted,
        })
    }
}

/// Workspace-scoped rust-analyzer session that owns the initialization lifecycle.
pub struct WorkspaceSession {
    session: Session,
    events: Receiver<SessionEvent>,
    buffered_events: VecDeque<SessionEvent>,
    phase: WorkspaceSessionPhase,
    workspace_root: PathBuf,
    workspace_uri: String,
    client_name: String,
    client_version: Option<String>,
    client_capabilities: Value,
    initialization_options: Value,
    workspace_configuration: Value,
    ready_timeout: Duration,
    ready_state: Option<WorkspaceReadyState>,
    loading_state: WorkspaceLoadingState,
}

impl WorkspaceSession {
    /// Returns the current lifecycle phase.
    pub fn phase(&self) -> WorkspaceSessionPhase {
        self.phase
    }

    /// Returns the workspace root for this session.
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    /// Returns the workspace root URI sent to the server.
    pub fn workspace_uri(&self) -> &str {
        &self.workspace_uri
    }

    /// Returns the most recently observed workspace loading state.
    pub fn loading_state(&self) -> &WorkspaceLoadingState {
        &self.loading_state
    }

    /// Returns the ready-state summary after initialization has completed.
    pub fn ready_state(&self) -> Option<&WorkspaceReadyState> {
        self.ready_state.as_ref()
    }

    /// Performs `initialize`, `initialized`, and the immediate startup configuration exchange.
    pub fn initialize(&mut self) -> Result<&WorkspaceReadyState, WorkspaceSessionError> {
        self.ensure_phase("initialize", WorkspaceSessionPhase::PreInitialize)?;
        self.phase = WorkspaceSessionPhase::Initializing;

        let result: Result<(), WorkspaceSessionError> = (|| {
            let initialize_result = self
                .session
                .request("initialize", self.initialize_params())?;
            let server_capabilities = initialize_result
                .get("capabilities")
                .cloned()
                .ok_or(WorkspaceSessionError::MissingInitializeField {
                    field: "capabilities",
                })?;
            let server_info = initialize_result.get("serverInfo").cloned();

            self.session.notify("initialized", json!({}))?;

            let mut configuration_requested = false;
            loop {
                match self.recv_event_with_timeout(self.ready_timeout)? {
                    Some(SessionEvent::ServerRequest(request))
                        if request.method == "workspace/configuration" =>
                    {
                        configuration_requested = true;
                        let response = configuration_response(
                            &self.workspace_configuration,
                            request.params.as_ref(),
                        );
                        self.session.respond(request.id, response)?;
                    }
                    Some(SessionEvent::Progress { value, .. }) => {
                        self.update_loading_state(&value);
                    }
                    Some(other) => {
                        self.capture_event(other);
                    }
                    None => break,
                }
            }

            self.phase = WorkspaceSessionPhase::Ready;
            self.ready_state = Some(WorkspaceReadyState {
                server_capabilities,
                server_info,
                configuration_requested,
                loading_state: self.loading_state.clone(),
            });

            Ok(())
        })();

        if result.is_err() {
            self.phase = WorkspaceSessionPhase::PreInitialize;
        }

        result?;
        Ok(self.ready_state.as_ref().expect("ready state set"))
    }

    /// Sends a request after the session reaches the ready phase.
    pub fn request<P>(&self, method: &str, params: P) -> Result<Value, WorkspaceSessionError>
    where
        P: Serialize,
    {
        self.ensure_ready("request")?;
        Ok(self.session.request(method, params)?)
    }

    /// Sends a notification after the session reaches the ready phase.
    pub fn notify<P>(&self, method: &str, params: P) -> Result<(), WorkspaceSessionError>
    where
        P: Serialize,
    {
        self.ensure_ready("notify")?;
        Ok(self.session.notify(method, params)?)
    }

    /// Returns the next buffered event or waits up to the provided timeout for a new one.
    pub fn next_event(
        &mut self,
        timeout: Duration,
    ) -> Result<Option<SessionEvent>, WorkspaceSessionError> {
        while let Some(event) = self.buffered_events.pop_front() {
            if !is_progress_notification(&event) {
                return Ok(Some(event));
            }
        }

        loop {
            match self.recv_event_with_timeout(timeout)? {
                Some(event) if is_progress_notification(&event) => continue,
                other => return Ok(other),
            }
        }
    }

    /// Performs the standard `shutdown` request and marks the workspace session as closed.
    pub fn shutdown(&mut self) -> Result<(), WorkspaceSessionError> {
        self.ensure_ready("shutdown")?;
        self.session.shutdown()?;
        self.phase = WorkspaceSessionPhase::Shutdown;
        Ok(())
    }

    fn initialize_params(&self) -> Value {
        json!({
            "processId": Value::Null,
            "clientInfo": {
                "name": self.client_name,
                "version": self.client_version,
            },
            "locale": "en-US",
            "rootPath": self.workspace_root.to_string_lossy(),
            "rootUri": self.workspace_uri,
            "workspaceFolders": [
                {
                    "uri": self.workspace_uri,
                    "name": workspace_folder_name(&self.workspace_root),
                }
            ],
            "trace": "off",
            "capabilities": self.client_capabilities,
            "initializationOptions": self.initialization_options,
        })
    }

    fn recv_event_with_timeout(
        &mut self,
        timeout: Duration,
    ) -> Result<Option<SessionEvent>, WorkspaceSessionError> {
        match self.events.recv_timeout(timeout) {
            Ok(event) => Ok(Some(event)),
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => Err(SessionError::Disconnected.into()),
        }
    }

    fn update_loading_state(&mut self, progress_value: &Value) {
        match progress_value.get("kind").and_then(Value::as_str) {
            Some("begin") | Some("report") => {
                let message = progress_value
                    .get("message")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
                self.loading_state = WorkspaceLoadingState::InProgress { message };
            }
            Some("end") => {
                self.loading_state = WorkspaceLoadingState::Ready;
            }
            _ => {}
        }
    }

    fn capture_event(&mut self, event: SessionEvent) {
        if let SessionEvent::Progress { value, .. } = &event {
            self.update_loading_state(value);
        }
        if is_progress_notification(&event) {
            return;
        }
        self.buffered_events.push_back(event);
    }

    fn ensure_phase(
        &self,
        operation: &'static str,
        phase: WorkspaceSessionPhase,
    ) -> Result<(), WorkspaceSessionError> {
        if self.phase == phase {
            Ok(())
        } else {
            Err(WorkspaceSessionError::InvalidPhase {
                operation,
                phase: self.phase,
            })
        }
    }

    fn ensure_ready(&self, operation: &'static str) -> Result<(), WorkspaceSessionError> {
        self.ensure_phase(operation, WorkspaceSessionPhase::Ready)
    }
}

fn default_client_capabilities() -> Value {
    json!({
        "general": {
            "positionEncodings": ["utf-8", "utf-16", "utf-32"],
        },
        "workspace": {
            "applyEdit": true,
            "configuration": true,
            "didChangeWatchedFiles": {
                "dynamicRegistration": true,
            },
            "workspaceEdit": {
                "resourceOperations": ["create", "rename", "delete"],
            },
        },
        "textDocument": {
            "codeAction": {
                "codeActionLiteralSupport": {
                    "codeActionKind": {
                        "valueSet": [
                            "",
                            "quickfix",
                            "refactor",
                            "refactor.extract",
                            "refactor.inline",
                            "refactor.rewrite",
                            "source",
                            "source.organizeImports",
                        ],
                    },
                },
            },
            "completion": {
                "completionItem": {
                    "snippetSupport": true,
                },
            },
            "publishDiagnostics": {
                "relatedInformation": true,
            },
        },
        "experimental": {
            "codeActionGroup": true,
            "hoverActions": true,
            "serverStatusNotification": true,
            "snippetTextEdit": true,
            "testExplorer": true,
        },
    })
}

fn default_initialization_options() -> Value {
    json!({
        "cargo": {
            "autoreload": true,
            "buildScripts": {
                "enable": true,
            },
        },
        "procMacro": {
            "enable": true,
        },
    })
}

fn default_workspace_configuration() -> Value {
    json!({
        "rust-analyzer": {
            "cargo": {
                "autoreload": true,
                "buildScripts": {
                    "enable": true,
                },
            },
            "checkOnSave": true,
            "files": {
                "watcher": "client",
            },
            "procMacro": {
                "enable": true,
            },
        },
    })
}

fn configuration_response(workspace_configuration: &Value, params: Option<&Value>) -> Value {
    let items = params
        .and_then(|params| params.get("items"))
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();

    Value::Array(
        items.into_iter()
            .map(|item| {
                let section = item.get("section").and_then(Value::as_str);
                lookup_config_section(workspace_configuration, section)
                    .cloned()
                    .unwrap_or(Value::Null)
            })
            .collect(),
    )
}

fn lookup_config_section<'a>(root: &'a Value, section: Option<&str>) -> Option<&'a Value> {
    match section {
        None | Some("") => Some(root),
        Some(section) => {
            let mut current = root;
            for part in section.split('.') {
                current = current.get(part)?;
            }
            Some(current)
        }
    }
}

fn absolutize_path(path: PathBuf) -> Result<PathBuf, SessionError> {
    if path.is_absolute() {
        Ok(path)
    } else {
        Ok(std::env::current_dir()?.join(path))
    }
}

fn file_uri_from_path(path: &Path) -> String {
    let mut uri = String::from("file://");

    for component in path.components() {
        match component {
            Component::Prefix(prefix) => match prefix.kind() {
                Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => {
                    uri.push('/');
                    uri.push(char::from(letter));
                    uri.push(':');
                }
                Prefix::UNC(server, share) | Prefix::VerbatimUNC(server, share) => {
                    uri.push_str(&percent_encode(server));
                    uri.push('/');
                    uri.push_str(&percent_encode(share));
                }
                Prefix::DeviceNS(namespace) => {
                    uri.push('/');
                    uri.push_str(&percent_encode(namespace));
                }
                Prefix::Verbatim(segment) => {
                    uri.push('/');
                    uri.push_str(&percent_encode(segment));
                }
            },
            Component::RootDir => uri.push('/'),
            Component::Normal(segment) => {
                if !uri.ends_with('/') {
                    uri.push('/');
                }
                uri.push_str(&percent_encode(segment));
            }
            Component::CurDir => {}
            Component::ParentDir => {
                if !uri.ends_with('/') {
                    uri.push('/');
                }
                uri.push_str("..");
            }
        }
    }

    uri
}

fn workspace_folder_name(path: &Path) -> String {
    path.file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("workspace")
        .to_owned()
}

fn percent_encode(segment: &OsStr) -> String {
    let bytes = segment.to_string_lossy();
    let mut encoded = String::new();
    for byte in bytes.as_bytes() {
        match byte {
            b'A'..=b'Z'
            | b'a'..=b'z'
            | b'0'..=b'9'
            | b'-'
            | b'_'
            | b'.'
            | b'~' => encoded.push(char::from(*byte)),
            _ => encoded.push_str(&format!("%{:02X}", byte)),
        }
    }
    encoded
}

fn is_progress_notification(event: &SessionEvent) -> bool {
    matches!(
        event,
        SessionEvent::Notification { method, .. } if method == "$/progress"
    )
}

#[cfg(test)]
mod tests {
    use super::file_uri_from_path;
    use std::path::Path;

    #[test]
    #[cfg(unix)]
    fn file_uri_from_unix_path_uses_forward_slashes() {
        assert_eq!(
            file_uri_from_path(Path::new("/tmp/rust workspace")),
            "file:///tmp/rust%20workspace"
        );
    }

    #[test]
    #[cfg(windows)]
    fn file_uri_from_windows_disk_path_uses_drive_prefix() {
        assert_eq!(
            file_uri_from_path(Path::new(r"C:\Users\dev\rust workspace")),
            "file:///C:/Users/dev/rust%20workspace"
        );
    }
}
