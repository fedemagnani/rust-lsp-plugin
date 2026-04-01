//! Workspace-scoped rust-analyzer session management built on top of the JSON-RPC transport.

use super::rust_analyzer as ra;
use super::session::{Session, SessionBuilder, SessionError, SessionEvent};
use lsp_types::{
    ClientCapabilities, ClientInfo, CompletionContext, CompletionParams, CompletionResponse,
    DidChangeTextDocumentParams, DidChangeWatchedFilesParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DidSaveTextDocumentParams, DocumentSymbolParams,
    DocumentSymbolResponse, FileEvent, GotoDefinitionParams, GotoDefinitionResponse, Hover,
    HoverParams, InitializeParams, InitializeResult, InitializedParams, Position,
    PrepareRenameResponse, ReferenceContext, ReferenceParams, RenameParams, ServerCapabilities,
    ServerInfo, TextDocumentContentChangeEvent, TextDocumentIdentifier, TextDocumentItem,
    TextDocumentPositionParams, TraceValue, Uri, VersionedTextDocumentIdentifier, WorkspaceEdit,
    WorkspaceFolder, WorkspaceSymbolParams, WorkspaceSymbolResponse,
    request::Request as LspRequest,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::collections::{HashMap, VecDeque};
use std::ffi::{OsStr, OsString};
use std::path::{Component, Path, PathBuf, Prefix};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;

const DEFAULT_READY_TIMEOUT: Duration = Duration::from_secs(1);
const WORKSPACE_PROGRESS_TOKEN: &str = "rustAnalyzer/workspace";

/// High-level session phase for the LSP initialization lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkspaceSessionPhase {
    /// The transport exists, but `initialize` has not been sent yet.
    PreInitialize,
    /// The client is performing the initialize and initialized handshake.
    Initializing,
    /// Initialization failed and the session can no longer be used safely.
    Failed,
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
    /// Server capabilities reported by initialize.
    pub server_capabilities: ServerCapabilities,
    /// Optional server information from the initialize response.
    pub server_info: Option<ServerInfo>,
    /// Whether the server requested workspace configuration during initialization.
    pub configuration_requested: bool,
    /// Workspace loading progress observed during the handshake.
    pub loading_state: WorkspaceLoadingState,
}

/// Locally tracked state for an open text document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrackedDocument {
    /// Absolute file-system path for the document.
    pub path: PathBuf,
    /// Canonical LSP text document item mirrored to the server.
    pub text_document: TextDocumentItem,
}

impl TrackedDocument {
    /// File URI sent to the server.
    pub fn uri(&self) -> &Uri {
        &self.text_document.uri
    }

    /// Language identifier used when the document was opened.
    pub fn language_id(&self) -> &str {
        &self.text_document.language_id
    }

    /// Most recent synchronized document version.
    pub fn version(&self) -> i32 {
        self.text_document.version
    }

    /// Most recent synchronized document contents.
    pub fn text(&self) -> &str {
        &self.text_document.text
    }
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
    /// The event receiver was not available when the workspace session was created.
    #[error("workspace session is missing the event receiver")]
    MissingEventReceiver,
    /// The requested document is not currently open.
    #[error("document is not open: {path}")]
    DocumentNotOpen {
        /// Absolute path for the document.
        path: PathBuf,
    },
    /// The requested document is already open.
    #[error("document is already open: {path}")]
    DocumentAlreadyOpen {
        /// Absolute path for the document.
        path: PathBuf,
    },
    /// A document version must increase monotonically.
    #[error(
        "document version must increase for {path}: current={current_version}, new={new_version}"
    )]
    NonMonotonicDocumentVersion {
        /// Absolute path for the document.
        path: PathBuf,
        /// Current synchronized version.
        current_version: i32,
        /// Proposed new version.
        new_version: i32,
    },
    /// A typed LSP response could not be decoded into the expected Rust shape.
    #[error("invalid typed response for {method}: {source}")]
    InvalidResponse {
        /// LSP method whose response failed to decode.
        method: &'static str,
        /// Underlying decode error.
        #[source]
        source: serde_json::Error,
    },
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
    client_capabilities: ClientCapabilities,
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
    pub fn client_capabilities(mut self, capabilities: ClientCapabilities) -> Self {
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
            open_documents: HashMap::new(),
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
    client_capabilities: ClientCapabilities,
    initialization_options: Value,
    workspace_configuration: Value,
    ready_timeout: Duration,
    ready_state: Option<WorkspaceReadyState>,
    loading_state: WorkspaceLoadingState,
    open_documents: HashMap<PathBuf, TrackedDocument>,
}

impl WorkspaceSession {
    /// Returns the current lifecycle phase.
    pub fn phase(&self) -> WorkspaceSessionPhase {
        self.phase
    }

    /// Returns whether the underlying transport session has terminated.
    pub fn is_disconnected(&self) -> bool {
        self.session.is_terminated()
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

    /// Returns the tracked state for an open document.
    pub fn document(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<Option<&TrackedDocument>, WorkspaceSessionError> {
        self.ensure_ready("document")?;
        let path = absolutize_path(path.as_ref().to_path_buf())?;
        Ok(self.open_documents.get(&path))
    }

    /// Returns all tracked open documents.
    pub fn open_documents(
        &self,
    ) -> Result<impl ExactSizeIterator<Item = &TrackedDocument>, WorkspaceSessionError> {
        self.ensure_ready("open_documents")?;
        Ok(self.open_documents.values())
    }

    /// Performs `initialize`, `initialized`, and the immediate startup configuration exchange.
    pub fn initialize(&mut self) -> Result<&WorkspaceReadyState, WorkspaceSessionError> {
        self.ensure_phase("initialize", WorkspaceSessionPhase::PreInitialize)?;
        self.phase = WorkspaceSessionPhase::Initializing;

        let result: Result<(), WorkspaceSessionError> = (|| {
            let initialize_result: InitializeResult = serde_json::from_value(
                self.session
                    .request("initialize", self.initialize_params())?,
            )
            .map_err(|source| WorkspaceSessionError::InvalidResponse {
                method: "initialize",
                source,
            })?;

            self.session.notify("initialized", InitializedParams {})?;

            let mut configuration_requested = false;
            loop {
                match self.recv_event_with_timeout(self.ready_timeout)? {
                    Some(SessionEvent::ServerRequest(request))
                        if request.method == "workspace/configuration" =>
                    {
                        configuration_requested = true;
                        let response = configuration_response(
                            &self.workspace_configuration,
                            Some(&request.params),
                        );
                        self.session.respond(request.id, response)?;
                    }
                    Some(SessionEvent::Progress { token, value }) => {
                        self.update_loading_state(&token, &value);
                    }
                    Some(other) => {
                        self.capture_event(other);
                    }
                    None => break,
                }
            }

            self.phase = WorkspaceSessionPhase::Ready;
            self.ready_state = Some(WorkspaceReadyState {
                server_capabilities: initialize_result.capabilities,
                server_info: initialize_result.server_info,
                configuration_requested,
                loading_state: self.loading_state.clone(),
            });

            Ok(())
        })();

        if result.is_err() {
            self.phase = WorkspaceSessionPhase::Failed;
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

    /// Sends a typed standard-LSP or rust-analyzer extension request.
    pub fn request_typed<R>(&self, params: R::Params) -> Result<R::Result, WorkspaceSessionError>
    where
        R: LspRequest,
        R::Params: Serialize,
        R::Result: DeserializeOwned,
    {
        self.typed_request::<R::Result, _>(R::METHOD, params)
    }

    /// Performs `textDocument/hover`.
    pub fn hover(
        &self,
        path: impl AsRef<Path>,
        position: Position,
    ) -> Result<Option<Hover>, WorkspaceSessionError> {
        self.typed_request(
            "textDocument/hover",
            HoverParams {
                text_document_position_params: text_document_position_params(
                    self.text_document_identifier(path)?,
                    position,
                ),
                work_done_progress_params: Default::default(),
            },
        )
    }

    /// Performs `textDocument/completion`.
    pub fn completion(
        &self,
        path: impl AsRef<Path>,
        position: Position,
        context: Option<CompletionContext>,
    ) -> Result<Option<CompletionResponse>, WorkspaceSessionError> {
        self.typed_request(
            "textDocument/completion",
            CompletionParams {
                text_document_position: text_document_position_params(
                    self.text_document_identifier(path)?,
                    position,
                ),
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
                context,
            },
        )
    }

    /// Performs `textDocument/definition`.
    pub fn goto_definition(
        &self,
        path: impl AsRef<Path>,
        position: Position,
    ) -> Result<Option<GotoDefinitionResponse>, WorkspaceSessionError> {
        self.typed_request(
            "textDocument/definition",
            GotoDefinitionParams {
                text_document_position_params: text_document_position_params(
                    self.text_document_identifier(path)?,
                    position,
                ),
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            },
        )
    }

    /// Performs `textDocument/references`.
    pub fn references(
        &self,
        path: impl AsRef<Path>,
        position: Position,
        include_declaration: bool,
    ) -> Result<Option<Vec<lsp_types::Location>>, WorkspaceSessionError> {
        self.typed_request(
            "textDocument/references",
            ReferenceParams {
                text_document_position: text_document_position_params(
                    self.text_document_identifier(path)?,
                    position,
                ),
                context: ReferenceContext {
                    include_declaration,
                },
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            },
        )
    }

    /// Performs `textDocument/prepareRename`.
    pub fn prepare_rename(
        &self,
        path: impl AsRef<Path>,
        position: Position,
    ) -> Result<Option<PrepareRenameResponse>, WorkspaceSessionError> {
        self.typed_request(
            "textDocument/prepareRename",
            text_document_position_params(self.text_document_identifier(path)?, position),
        )
    }

    /// Performs `textDocument/rename`.
    pub fn rename(
        &self,
        path: impl AsRef<Path>,
        position: Position,
        new_name: impl Into<String>,
    ) -> Result<Option<WorkspaceEdit>, WorkspaceSessionError> {
        self.typed_request(
            "textDocument/rename",
            RenameParams {
                text_document_position: text_document_position_params(
                    self.text_document_identifier(path)?,
                    position,
                ),
                new_name: new_name.into(),
                work_done_progress_params: Default::default(),
            },
        )
    }

    /// Performs `textDocument/documentSymbol`.
    pub fn document_symbols(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<Option<DocumentSymbolResponse>, WorkspaceSessionError> {
        self.typed_request(
            "textDocument/documentSymbol",
            DocumentSymbolParams {
                text_document: self.text_document_identifier(path)?,
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            },
        )
    }

    /// Performs `workspace/symbol`.
    pub fn workspace_symbols(
        &self,
        query: impl Into<String>,
    ) -> Result<Option<WorkspaceSymbolResponse>, WorkspaceSessionError> {
        self.request_typed::<lsp_types::request::WorkspaceSymbolRequest>(WorkspaceSymbolParams {
            query: query.into(),
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        })
    }

    /// Performs `rust-analyzer/analyzerStatus`.
    pub fn analyzer_status(
        &self,
        path: Option<impl AsRef<Path>>,
    ) -> Result<String, WorkspaceSessionError> {
        self.request_typed::<ra::AnalyzerStatus>(ra::AnalyzerStatusParams {
            text_document: match path {
                Some(path) => Some(self.text_document_identifier(path)?),
                None => None,
            },
        })
    }

    /// Performs `rust-analyzer/fetchDependencyList`.
    pub fn fetch_dependency_list(
        &self,
    ) -> Result<ra::FetchDependencyListResult, WorkspaceSessionError> {
        self.request_typed::<ra::FetchDependencyList>(ra::FetchDependencyListParams::default())
    }

    /// Performs `rust-analyzer/reloadWorkspace`.
    pub fn reload_workspace(&self) -> Result<(), WorkspaceSessionError> {
        self.request_typed::<ra::ReloadWorkspace>(())
    }

    /// Performs `rust-analyzer/rebuildProcMacros`.
    pub fn rebuild_proc_macros(&self) -> Result<(), WorkspaceSessionError> {
        self.request_typed::<ra::RebuildProcMacros>(())
    }

    /// Performs `rust-analyzer/viewSyntaxTree`.
    pub fn view_syntax_tree(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<String, WorkspaceSessionError> {
        self.request_typed::<ra::ViewSyntaxTree>(ra::ViewSyntaxTreeParams {
            text_document: self.text_document_identifier(path)?,
        })
    }

    /// Performs `rust-analyzer/viewHir`.
    pub fn view_hir(
        &self,
        path: impl AsRef<Path>,
        position: Position,
    ) -> Result<String, WorkspaceSessionError> {
        self.request_typed::<ra::ViewHir>(text_document_position_params(
            self.text_document_identifier(path)?,
            position,
        ))
    }

    /// Performs `rust-analyzer/viewMir`.
    pub fn view_mir(
        &self,
        path: impl AsRef<Path>,
        position: Position,
    ) -> Result<String, WorkspaceSessionError> {
        self.request_typed::<ra::ViewMir>(text_document_position_params(
            self.text_document_identifier(path)?,
            position,
        ))
    }

    /// Performs `rust-analyzer/expandMacro`.
    pub fn expand_macro(
        &self,
        path: impl AsRef<Path>,
        position: Position,
    ) -> Result<Option<ra::ExpandedMacro>, WorkspaceSessionError> {
        self.request_typed::<ra::ExpandMacro>(ra::ExpandMacroParams {
            text_document: self.text_document_identifier(path)?,
            position,
        })
    }

    /// Performs `experimental/runnables`.
    pub fn runnables(
        &self,
        path: impl AsRef<Path>,
        position: Option<Position>,
    ) -> Result<Vec<ra::Runnable>, WorkspaceSessionError> {
        self.request_typed::<ra::Runnables>(ra::RunnablesParams {
            text_document: self.text_document_identifier(path)?,
            position,
        })
    }

    /// Performs `rust-analyzer/relatedTests`.
    pub fn related_tests(
        &self,
        path: impl AsRef<Path>,
        position: Position,
    ) -> Result<Vec<ra::TestInfo>, WorkspaceSessionError> {
        self.request_typed::<ra::RelatedTests>(text_document_position_params(
            self.text_document_identifier(path)?,
            position,
        ))
    }

    /// Sends a notification after the session reaches the ready phase.
    pub fn notify<P>(&self, method: &str, params: P) -> Result<(), WorkspaceSessionError>
    where
        P: Serialize,
    {
        self.ensure_ready("notify")?;
        Ok(self.session.notify(method, params)?)
    }

    /// Opens a document and starts synchronizing its contents with the server.
    pub fn open_document(
        &mut self,
        path: impl AsRef<Path>,
        language_id: impl Into<String>,
        version: i32,
        text: impl Into<String>,
    ) -> Result<&TrackedDocument, WorkspaceSessionError> {
        self.ensure_ready("open_document")?;

        let path = absolutize_path(path.as_ref().to_path_buf())?;
        if self.open_documents.contains_key(&path) {
            return Err(WorkspaceSessionError::DocumentAlreadyOpen { path });
        }

        let tracked = TrackedDocument {
            path: path.clone(),
            text_document: TextDocumentItem::new(
                parse_uri(&file_uri_from_path(&path)),
                language_id.into(),
                version,
                text.into(),
            ),
        };

        self.session.notify(
            "textDocument/didOpen",
            DidOpenTextDocumentParams {
                text_document: tracked.text_document.clone(),
            },
        )?;

        self.open_documents.insert(path.clone(), tracked);
        Ok(self.open_documents.get(&path).expect("document inserted"))
    }

    /// Applies a full-document text update to an open document.
    pub fn change_document(
        &mut self,
        path: impl AsRef<Path>,
        version: i32,
        text: impl Into<String>,
    ) -> Result<&TrackedDocument, WorkspaceSessionError> {
        self.ensure_ready("change_document")?;

        let path = absolutize_path(path.as_ref().to_path_buf())?;
        let tracked = self
            .open_documents
            .get_mut(&path)
            .ok_or_else(|| WorkspaceSessionError::DocumentNotOpen { path: path.clone() })?;

        if version <= tracked.version() {
            return Err(WorkspaceSessionError::NonMonotonicDocumentVersion {
                path,
                current_version: tracked.version(),
                new_version: version,
            });
        }

        let text = text.into();
        self.session.notify(
            "textDocument/didChange",
            DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier::new(
                    tracked.text_document.uri.clone(),
                    version,
                ),
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: text.clone(),
                }],
            },
        )?;

        tracked.text_document.version = version;
        tracked.text_document.text = text;
        Ok(tracked)
    }

    /// Replaces the full contents of an open document, auto-incrementing the version.
    pub fn replace_document(
        &mut self,
        path: impl AsRef<Path>,
        text: impl Into<String>,
    ) -> Result<&TrackedDocument, WorkspaceSessionError> {
        self.ensure_ready("replace_document")?;

        let path = absolutize_path(path.as_ref().to_path_buf())?;
        let tracked = self
            .open_documents
            .get_mut(&path)
            .ok_or_else(|| WorkspaceSessionError::DocumentNotOpen { path: path.clone() })?;

        let next_version = tracked.version() + 1;
        let text = text.into();
        self.session.notify(
            "textDocument/didChange",
            DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier::new(
                    tracked.text_document.uri.clone(),
                    next_version,
                ),
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: text.clone(),
                }],
            },
        )?;

        tracked.text_document.version = next_version;
        tracked.text_document.text = text;
        Ok(tracked)
    }

    /// Notifies the server that an open document was saved.
    pub fn save_document(
        &mut self,
        path: impl AsRef<Path>,
    ) -> Result<&TrackedDocument, WorkspaceSessionError> {
        self.ensure_ready("save_document")?;

        let path = absolutize_path(path.as_ref().to_path_buf())?;
        let tracked = self
            .open_documents
            .get(&path)
            .ok_or_else(|| WorkspaceSessionError::DocumentNotOpen { path: path.clone() })?;

        self.session.notify(
            "textDocument/didSave",
            DidSaveTextDocumentParams {
                text_document: TextDocumentIdentifier::new(tracked.text_document.uri.clone()),
                text: None,
            },
        )?;

        Ok(tracked)
    }

    /// Stops synchronizing an open document and removes its local tracked state.
    pub fn close_document(
        &mut self,
        path: impl AsRef<Path>,
    ) -> Result<TrackedDocument, WorkspaceSessionError> {
        self.ensure_ready("close_document")?;

        let path = absolutize_path(path.as_ref().to_path_buf())?;
        let tracked = self
            .open_documents
            .get(&path)
            .ok_or_else(|| WorkspaceSessionError::DocumentNotOpen { path: path.clone() })?;

        self.session.notify(
            "textDocument/didClose",
            DidCloseTextDocumentParams {
                text_document: TextDocumentIdentifier::new(tracked.text_document.uri.clone()),
            },
        )?;

        Ok(self
            .open_documents
            .remove(&path)
            .expect("document verified present"))
    }

    /// Pushes a workspace configuration change notification.
    pub fn change_configuration(
        &self,
        settings: impl Serialize,
    ) -> Result<(), WorkspaceSessionError> {
        self.ensure_ready("change_configuration")?;
        self.session.notify(
            "workspace/didChangeConfiguration",
            json!({ "settings": settings }),
        )?;
        Ok(())
    }

    /// Pushes watched-file changes that can affect workspace analysis.
    pub fn change_watched_files(
        &self,
        changes: impl IntoIterator<Item = FileEvent>,
    ) -> Result<(), WorkspaceSessionError> {
        self.ensure_ready("change_watched_files")?;
        let changes = changes.into_iter().collect::<Vec<_>>();

        self.session.notify(
            "workspace/didChangeWatchedFiles",
            DidChangeWatchedFilesParams { changes },
        )?;
        Ok(())
    }

    /// Drains progress events until the workspace loading state reaches `Ready` or the timeout
    /// elapses. Returns `true` if the workspace finished loading, `false` on timeout.
    pub fn wait_until_loaded(&mut self, timeout: Duration) -> bool {
        if self.loading_state == WorkspaceLoadingState::Ready {
            return true;
        }

        let deadline = std::time::Instant::now() + timeout;
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            if remaining.is_zero() {
                return self.loading_state == WorkspaceLoadingState::Ready;
            }

            match self.recv_event_with_timeout(remaining) {
                Ok(Some(SessionEvent::ServerRequest(request)))
                    if request.method == "workspace/configuration" =>
                {
                    let response = configuration_response(
                        &self.workspace_configuration,
                        Some(&request.params),
                    );
                    if self.session.respond(request.id, response).is_err() {
                        return false;
                    }
                }
                Ok(Some(event)) => {
                    self.capture_event(event);
                    if self.loading_state == WorkspaceLoadingState::Ready {
                        return true;
                    }
                }
                Ok(None) => {
                    return self.loading_state == WorkspaceLoadingState::Ready;
                }
                Err(_) => {
                    return false;
                }
            }
        }
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

    #[allow(deprecated)]
    fn initialize_params(&self) -> InitializeParams {
        InitializeParams {
            process_id: None,
            root_path: Some(self.workspace_root.to_string_lossy().into_owned()),
            root_uri: Some(parse_uri(&self.workspace_uri)),
            initialization_options: Some(self.initialization_options.clone()),
            capabilities: self.client_capabilities.clone(),
            trace: Some(TraceValue::Off),
            workspace_folders: Some(vec![WorkspaceFolder {
                uri: parse_uri(&self.workspace_uri),
                name: workspace_folder_name(&self.workspace_root),
            }]),
            client_info: Some(ClientInfo {
                name: self.client_name.clone(),
                version: self.client_version.clone(),
            }),
            locale: Some("en-US".to_owned()),
            work_done_progress_params: Default::default(),
        }
    }

    fn recv_event_with_timeout(
        &mut self,
        timeout: Duration,
    ) -> Result<Option<SessionEvent>, WorkspaceSessionError> {
        match self.events.recv_timeout(timeout) {
            Ok(event) => Ok(Some(event)),
            Err(RecvTimeoutError::Timeout) if self.session.is_terminated() => {
                Err(SessionError::Disconnected.into())
            }
            Err(RecvTimeoutError::Timeout) => Ok(None),
            Err(RecvTimeoutError::Disconnected) => Err(SessionError::Disconnected.into()),
        }
    }

    fn update_loading_state(&mut self, token: &Value, progress_value: &Value) {
        if token != &Value::String(WORKSPACE_PROGRESS_TOKEN.to_owned()) {
            return;
        }

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
        if let SessionEvent::Progress { token, value } = &event {
            self.update_loading_state(token, value);
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

    fn document_uri(&self, path: impl AsRef<Path>) -> Result<String, WorkspaceSessionError> {
        let path = absolutize_path(path.as_ref().to_path_buf())?;
        Ok(self
            .open_documents
            .get(&path)
            .map(|document| document.uri().as_str().to_owned())
            .unwrap_or_else(|| file_uri_from_path(&path)))
    }

    fn text_document_identifier(
        &self,
        path: impl AsRef<Path>,
    ) -> Result<TextDocumentIdentifier, WorkspaceSessionError> {
        let uri = self.document_uri(path)?;
        Ok(TextDocumentIdentifier {
            uri: parse_uri(&uri),
        })
    }

    fn typed_request<R, P>(
        &self,
        method: &'static str,
        params: P,
    ) -> Result<R, WorkspaceSessionError>
    where
        R: DeserializeOwned,
        P: Serialize,
    {
        let response = self.request(method, params)?;
        serde_json::from_value(response)
            .map_err(|source| WorkspaceSessionError::InvalidResponse { method, source })
    }
}

fn parse_uri(uri: &str) -> Uri {
    uri.parse().expect("generated file URI is valid")
}

fn text_document_position_params(
    text_document: TextDocumentIdentifier,
    position: Position,
) -> TextDocumentPositionParams {
    TextDocumentPositionParams::new(text_document, position)
}

fn default_client_capabilities() -> ClientCapabilities {
    serde_json::from_value(json!({
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
            "synchronization": {
                "didSave": true,
                "dynamicRegistration": false,
                "willSave": false,
                "willSaveWaitUntil": false,
            },
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
    }))
    .expect("default client capabilities are valid")
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
        items
            .into_iter()
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
    let absolute = if path.is_absolute() {
        path
    } else {
        std::env::current_dir()?.join(path)
    };

    let components = absolute.components().collect::<Vec<_>>();

    for split in (0..=components.len()).rev() {
        let mut prefix = PathBuf::new();
        for component in &components[..split] {
            prefix.push(component.as_os_str());
        }

        match prefix.canonicalize() {
            Ok(mut canonical) => {
                for component in &components[split..] {
                    canonical.push(component.as_os_str());
                }
                return Ok(normalize_path(canonical));
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(SessionError::Io(error)),
        }
    }

    Ok(normalize_path(absolute))
}

fn normalize_path(path: PathBuf) -> PathBuf {
    let mut normalized = PathBuf::new();

    for component in path.components() {
        match component {
            Component::Prefix(prefix) => normalized.push(prefix.as_os_str()),
            Component::RootDir => normalized.push(Path::new(std::path::MAIN_SEPARATOR_STR)),
            Component::CurDir => {}
            Component::ParentDir => {
                if !normalized.pop() && !path.is_absolute() {
                    normalized.push("..");
                }
            }
            Component::Normal(segment) => normalized.push(segment),
        }
    }

    normalized
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
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(char::from(*byte))
            }
            _ => encoded.push_str(&format!("%{:02X}", byte)),
        }
    }
    encoded
}

fn is_progress_notification(event: &SessionEvent) -> bool {
    matches!(
        event,
        SessionEvent::Notification(notification) if notification.method == "$/progress"
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
