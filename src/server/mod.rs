//! MCP server runtime scaffolding built on `rmcp`.

mod error;
mod schema;

use crate::{
    CreateFile, DeleteFile, DocumentChangeOperation, DocumentChanges,
    GotoDefinitionResponse, HoverContents, MarkedString, MarkupKind, OneOf, Position, Range,
    RenameFile, ResourceOp, SymbolInformation, SymbolKind, TextEdit, Uri, WorkspaceEdit,
    WorkspaceLocation, WorkspaceSession, WorkspaceSessionBuilder, WorkspaceSessionError,
    WorkspaceSessionPhase, WorkspaceSymbol, WorkspaceSymbolResponse,
};
use rmcp::handler::server::wrapper::Parameters;
use rmcp::handler::server::router::tool::ToolRouter;
use rmcp::model::{ProtocolVersion, ServerCapabilities, ServerInfo};
use rmcp::transport::stdio;
use rmcp::{ErrorData, Json, ServiceExt, ServerHandler};
use rmcp_macros::{tool, tool_handler, tool_router};
use std::error::Error;
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, RwLock};
use std::time::Duration;
use tokio::task;

pub use error::{ServerError, ServerErrorKind};
pub use schema::*;

/// Fallible result used by the MCP server runtime.
pub type ServerResult<T> = Result<T, Box<dyn Error + Send + Sync>>;

const DEFAULT_WORKSPACE_READY_TIMEOUT: Duration = Duration::from_secs(1);

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
///
/// The server manages at most one rust-analyzer session at a time. When a tool call targets a
/// different workspace root, the previous session is shut down and replaced.
#[derive(Default)]
pub struct ServerState {
    session_config: RwLock<Option<WorkspaceSessionConfig>>,
    workspace: Mutex<Option<(PathBuf, WorkspaceSession)>>,
}

impl std::fmt::Debug for ServerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let active_root = self.active_workspace_root();
        f.debug_struct("ServerState")
            .field("active_workspace_root", &active_root)
            .finish()
    }
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

    /// Returns the currently active workspace root, if any.
    pub fn active_workspace_root(&self) -> Option<PathBuf> {
        self.workspace
            .lock()
            .expect("workspace poisoned")
            .as_ref()
            .map(|(root, _)| root.clone())
    }

    /// Routes work to the workspace session, creating or replacing it on demand.
    pub fn with_workspace_session<T, F>(
        &self,
        root: impl AsRef<Path>,
        operation: &'static str,
        f: F,
    ) -> Result<T, ServerError>
    where
        F: FnOnce(&mut WorkspaceSession) -> Result<T, WorkspaceSessionError>,
    {
        let root = normalize_workspace_root(root.as_ref())?;
        let config = self
            .workspace_session_config()
            .ok_or_else(|| ServerError::internal("workspace session config is not set"))?;

        let mut slot = self.workspace.lock().expect("workspace poisoned");

        let must_spawn = match slot.as_ref() {
            None => true,
            Some((current_root, session)) => {
                *current_root != root
                    || matches!(
                        session.phase(),
                        WorkspaceSessionPhase::Failed | WorkspaceSessionPhase::Shutdown
                    )
                    || session.is_disconnected()
            }
        };

        if must_spawn {
            // Gracefully shut down the old session before replacing it.
            if let Some((_, mut old_session)) = slot.take() {
                let _ = old_session.shutdown();
            }
            let session = config
                .spawn_initialized(&root)
                .map_err(ServerError::from)?;
            *slot = Some((root.clone(), session));
        }

        let (_, session) = slot
            .as_mut()
            .expect("workspace session initialized before routing");
        f(session).map_err(|error| ServerError::from(error).with_operation(operation))
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

    async fn with_workspace_session_blocking<T, F>(
        &self,
        root: PathBuf,
        operation: &'static str,
        f: F,
    ) -> Result<T, ErrorData>
    where
        T: Send + 'static,
        F: FnOnce(&mut WorkspaceSession) -> Result<T, WorkspaceSessionError> + Send + 'static,
    {
        let state = Arc::clone(&self.state);
        task::spawn_blocking(move || state.with_workspace_session(root, operation, f))
            .await
            .map_err(|error| {
                ErrorData::from(
                    ServerError::internal(format!("workspace operation task failed: {error}"))
                        .with_operation(operation),
                )
            })?
            .map_err(ErrorData::from)
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

fn normalize_workspace_root(root: &Path) -> Result<PathBuf, ServerError> {
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

fn to_lsp_position(position: TextPosition) -> Position {
    Position::new(position.line, position.character)
}

fn normalize_hover(hover: crate::Hover) -> HoverSummary {
    HoverSummary {
        contents: normalize_hover_contents(hover.contents),
        range: hover.range.map(normalize_range),
    }
}

fn normalize_hover_contents(contents: HoverContents) -> HoverContent {
    match contents {
        HoverContents::Scalar(marked) => HoverContent::Markdown(normalize_marked_string(marked)),
        HoverContents::Array(items) => HoverContent::Markdown(
            items
                .into_iter()
                .map(normalize_marked_string)
                .collect::<Vec<_>>()
                .join("\n\n"),
        ),
        HoverContents::Markup(markup) => {
            if markup.kind == MarkupKind::PlainText {
                HoverContent::PlainText(markup.value)
            } else {
                HoverContent::Markdown(markup.value)
            }
        }
    }
}

fn normalize_marked_string(value: MarkedString) -> String {
    match value {
        MarkedString::String(text) => text,
        MarkedString::LanguageString(language) => {
            format!("```{}\n{}\n```", language.language, language.value)
        }
    }
}

fn normalize_definitions(definitions: Option<GotoDefinitionResponse>) -> Vec<DocumentLocation> {
    match definitions {
        None => Vec::new(),
        Some(GotoDefinitionResponse::Scalar(location)) => vec![normalize_location(location)],
        Some(GotoDefinitionResponse::Array(locations)) => {
            locations.into_iter().map(normalize_location).collect()
        }
        Some(GotoDefinitionResponse::Link(links)) => links
            .into_iter()
            .map(|link| DocumentLocation {
                document_path: uri_to_path(&link.target_uri),
                range: normalize_range(link.target_selection_range),
            })
            .collect(),
    }
}

fn normalize_workspace_symbols(symbols: Option<WorkspaceSymbolResponse>) -> Vec<SymbolSummary> {
    match symbols {
        None => Vec::new(),
        Some(WorkspaceSymbolResponse::Flat(symbols)) => symbols
            .into_iter()
            .map(normalize_flat_symbol)
            .collect(),
        Some(WorkspaceSymbolResponse::Nested(symbols)) => symbols
            .into_iter()
            .map(normalize_nested_symbol)
            .collect(),
    }
}

fn normalize_flat_symbol(symbol: SymbolInformation) -> SymbolSummary {
    SymbolSummary {
        name: symbol.name,
        kind: symbol_kind_name(symbol.kind),
        container_name: symbol.container_name,
        location: normalize_location(symbol.location),
    }
}

fn normalize_nested_symbol(symbol: WorkspaceSymbol) -> SymbolSummary {
    let location = match symbol.location {
        OneOf::Left(location) => normalize_location(location),
        OneOf::Right(WorkspaceLocation { uri }) => DocumentLocation {
            document_path: uri_to_path(&uri),
            range: TextRange {
                start: TextPosition {
                    line: 0,
                    character: 0,
                },
                end: TextPosition {
                    line: 0,
                    character: 0,
                },
            },
        },
    };

    SymbolSummary {
        name: symbol.name,
        kind: symbol_kind_name(symbol.kind),
        container_name: symbol.container_name,
        location,
    }
}

fn symbol_kind_name(kind: SymbolKind) -> String {
    if kind == SymbolKind::FILE {
        "file".to_owned()
    } else if kind == SymbolKind::MODULE {
        "module".to_owned()
    } else if kind == SymbolKind::NAMESPACE {
        "namespace".to_owned()
    } else if kind == SymbolKind::PACKAGE {
        "package".to_owned()
    } else if kind == SymbolKind::CLASS {
        "class".to_owned()
    } else if kind == SymbolKind::METHOD {
        "method".to_owned()
    } else if kind == SymbolKind::PROPERTY {
        "property".to_owned()
    } else if kind == SymbolKind::FIELD {
        "field".to_owned()
    } else if kind == SymbolKind::CONSTRUCTOR {
        "constructor".to_owned()
    } else if kind == SymbolKind::ENUM {
        "enum".to_owned()
    } else if kind == SymbolKind::INTERFACE {
        "interface".to_owned()
    } else if kind == SymbolKind::FUNCTION {
        "function".to_owned()
    } else if kind == SymbolKind::VARIABLE {
        "variable".to_owned()
    } else if kind == SymbolKind::CONSTANT {
        "constant".to_owned()
    } else if kind == SymbolKind::STRING {
        "string".to_owned()
    } else if kind == SymbolKind::NUMBER {
        "number".to_owned()
    } else if kind == SymbolKind::BOOLEAN {
        "boolean".to_owned()
    } else if kind == SymbolKind::ARRAY {
        "array".to_owned()
    } else if kind == SymbolKind::OBJECT {
        "object".to_owned()
    } else if kind == SymbolKind::KEY {
        "key".to_owned()
    } else if kind == SymbolKind::NULL {
        "null".to_owned()
    } else if kind == SymbolKind::ENUM_MEMBER {
        "enum_member".to_owned()
    } else if kind == SymbolKind::STRUCT {
        "struct".to_owned()
    } else if kind == SymbolKind::EVENT {
        "event".to_owned()
    } else if kind == SymbolKind::OPERATOR {
        "operator".to_owned()
    } else if kind == SymbolKind::TYPE_PARAMETER {
        "type_parameter".to_owned()
    } else {
        serde_json::to_value(kind)
            .ok()
            .and_then(|value| value.as_i64().map(|raw| format!("unknown_{raw}")))
            .unwrap_or_else(|| "unknown".to_owned())
    }
}

fn normalize_location(location: crate::Location) -> DocumentLocation {
    DocumentLocation {
        document_path: uri_to_path(&location.uri),
        range: normalize_range(location.range),
    }
}

fn normalize_range(range: Range) -> TextRange {
    TextRange {
        start: TextPosition {
            line: range.start.line,
            character: range.start.character,
        },
        end: TextPosition {
            line: range.end.line,
            character: range.end.character,
        },
    }
}

fn uri_to_path(uri: &Uri) -> PathBuf {
    let raw = uri.as_str();
    raw.strip_prefix("file://")
        .map(percent_decode_path)
        .unwrap_or_else(|| PathBuf::from(raw))
}

fn percent_decode_path(path: &str) -> PathBuf {
    let bytes = path.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;

    while index < bytes.len() {
        if bytes[index] == b'%' && index + 2 < bytes.len() {
            let high = from_hex_digit(bytes[index + 1]);
            let low = from_hex_digit(bytes[index + 2]);
            if let (Some(high), Some(low)) = (high, low) {
                decoded.push((high << 4) | low);
                index += 3;
                continue;
            }
        }

        decoded.push(bytes[index]);
        index += 1;
    }

    PathBuf::from(String::from_utf8_lossy(&decoded).into_owned())
}

fn from_hex_digit(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

fn normalize_workspace_edit(edit: WorkspaceEdit) -> WorkspaceEditSummary {
    let mut changes = Vec::new();

    if let Some(document_changes) = edit.document_changes {
        match document_changes {
            DocumentChanges::Edits(edits) => {
                for edit in edits {
                    changes.push(WorkspaceChangeSummary::TextEdits {
                        document_path: uri_to_path(&edit.text_document.uri),
                        edits: edit
                            .edits
                            .into_iter()
                            .map(|e| match e {
                                OneOf::Left(text_edit) => normalize_text_edit(text_edit),
                                OneOf::Right(annotated) => normalize_text_edit(annotated.text_edit),
                            })
                            .collect(),
                    });
                }
            }
            DocumentChanges::Operations(ops) => {
                for op in ops {
                    match op {
                        DocumentChangeOperation::Edit(edit) => {
                            changes.push(WorkspaceChangeSummary::TextEdits {
                                document_path: uri_to_path(&edit.text_document.uri),
                                edits: edit
                                    .edits
                                    .into_iter()
                                    .map(|e| match e {
                                        OneOf::Left(text_edit) => {
                                            normalize_text_edit(text_edit)
                                        }
                                        OneOf::Right(annotated) => {
                                            normalize_text_edit(annotated.text_edit)
                                        }
                                    })
                                    .collect(),
                            });
                        }
                        DocumentChangeOperation::Op(op) => match op {
                            ResourceOp::Create(CreateFile { uri, .. }) => {
                                changes.push(WorkspaceChangeSummary::CreateFile {
                                    path: uri_to_path(&uri),
                                });
                            }
                            ResourceOp::Rename(RenameFile {
                                old_uri, new_uri, ..
                            }) => {
                                changes.push(WorkspaceChangeSummary::RenameFile {
                                    old_path: uri_to_path(&old_uri),
                                    new_path: uri_to_path(&new_uri),
                                });
                            }
                            ResourceOp::Delete(DeleteFile { uri, .. }) => {
                                changes.push(WorkspaceChangeSummary::DeleteFile {
                                    path: uri_to_path(&uri),
                                });
                            }
                        },
                    }
                }
            }
        }
    } else if let Some(text_changes) = edit.changes {
        for (uri, edits) in text_changes {
            changes.push(WorkspaceChangeSummary::TextEdits {
                document_path: uri_to_path(&uri),
                edits: edits.into_iter().map(normalize_text_edit).collect(),
            });
        }
    }

    WorkspaceEditSummary { changes }
}

fn normalize_text_edit(edit: TextEdit) -> TextEditSummary {
    TextEditSummary {
        range: normalize_range(edit.range),
        new_text: edit.new_text,
    }
}

#[tool_router]
impl RustAnalyzerMcpServer {
    #[tool(
        name = "hover",
        description = "Inspect hover information for a symbol in a workspace document.",
        annotations(
            title = "Hover",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn hover(
        &self,
        params: Parameters<DocumentPositionInput>,
    ) -> Result<Json<ReadOnlyToolResult<Option<HoverSummary>>>, ErrorData> {
        let params = params.0;
        let workspace_root = params.workspace_root;
        let document_path = params.document_path;
        let position = to_lsp_position(params.position);
        let result = self
            .with_workspace_session_blocking(workspace_root, "hover", move |session| {
                session.hover(&document_path, position)
            })
            .await?;

        Ok(Json(ReadOnlyToolResult {
            data: result.map(normalize_hover),
            execution: Default::default(),
        }))
    }

    #[tool(
        name = "definitions",
        description = "Resolve definition locations for a symbol in a workspace document.",
        annotations(
            title = "Definitions",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn definitions(
        &self,
        params: Parameters<DocumentPositionInput>,
    ) -> Result<Json<ReadOnlyToolResult<Vec<DocumentLocation>>>, ErrorData> {
        let params = params.0;
        let workspace_root = params.workspace_root;
        let document_path = params.document_path;
        let position = to_lsp_position(params.position);
        let result = self
            .with_workspace_session_blocking(workspace_root, "definitions", move |session| {
                session.goto_definition(&document_path, position)
            })
            .await?;

        Ok(Json(ReadOnlyToolResult {
            data: normalize_definitions(result),
            execution: Default::default(),
        }))
    }

    #[tool(
        name = "references",
        description = "List references for a symbol in a workspace document.",
        annotations(
            title = "References",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn references(
        &self,
        params: Parameters<DocumentPositionInput>,
    ) -> Result<Json<ReadOnlyToolResult<Vec<DocumentLocation>>>, ErrorData> {
        let params = params.0;
        let workspace_root = params.workspace_root;
        let document_path = params.document_path;
        let position = to_lsp_position(params.position);
        let result = self
            .with_workspace_session_blocking(workspace_root, "references", move |session| {
                session.references(&document_path, position, true)
            })
            .await?;

        Ok(Json(ReadOnlyToolResult {
            data: result
                .unwrap_or_default()
                .into_iter()
                .map(normalize_location)
                .collect(),
            execution: Default::default(),
        }))
    }

    #[tool(
        name = "workspace_symbols",
        description = "Search for symbols in a registered workspace root.",
        annotations(
            title = "Workspace Symbols",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn workspace_symbols(
        &self,
        params: Parameters<WorkspaceQueryInput>,
    ) -> Result<Json<ReadOnlyToolResult<Vec<SymbolSummary>>>, ErrorData> {
        let params = params.0;
        let workspace_root = params.workspace_root;
        let query = params.query;
        let result = self
            .with_workspace_session_blocking(workspace_root, "workspace_symbols", move |session| {
                session.workspace_symbols(query)
            })
            .await?;

        Ok(Json(ReadOnlyToolResult {
            data: normalize_workspace_symbols(result),
            execution: Default::default(),
        }))
    }

    #[tool(
        name = "analyzer_status",
        description = "Read the rust-analyzer status string for a document or workspace.",
        annotations(
            title = "Analyzer Status",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn analyzer_status(
        &self,
        params: Parameters<AnalyzerStatusInput>,
    ) -> Result<Json<ReadOnlyToolResult<AnalyzerStatusSummary>>, ErrorData> {
        let params = params.0;
        let workspace_root = params.workspace_root;
        let document_path = params.document_path;
        let status = self
            .with_workspace_session_blocking(workspace_root, "analyzer_status", move |session| {
                session.analyzer_status(document_path.as_ref())
            })
            .await?;

        Ok(Json(ReadOnlyToolResult {
            data: AnalyzerStatusSummary { status },
            execution: Default::default(),
        }))
    }

    #[tool(
        name = "view_syntax_tree",
        description = "Inspect the syntax tree for a document in a registered workspace root.",
        annotations(
            title = "Syntax Tree",
            read_only_hint = true,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn view_syntax_tree(
        &self,
        params: Parameters<DocumentInput>,
    ) -> Result<Json<ReadOnlyToolResult<SyntaxTreeSummary>>, ErrorData> {
        let params = params.0;
        let workspace_root = params.workspace_root;
        let document_path = params.document_path;
        let tree = self
            .with_workspace_session_blocking(workspace_root, "view_syntax_tree", move |session| {
                session.view_syntax_tree(&document_path)
            })
            .await?;

        Ok(Json(ReadOnlyToolResult {
            data: SyntaxTreeSummary { tree },
            execution: Default::default(),
        }))
    }

    #[tool(
        name = "open_document",
        description = "Open a document and synchronize its contents with the workspace rust-analyzer session. The document must be opened before position-based tools can operate on it.",
        annotations(
            title = "Open Document",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn open_document(
        &self,
        params: Parameters<OpenDocumentInput>,
    ) -> Result<Json<MutatingToolResult<OpenDocumentSummary>>, ErrorData> {
        let params = params.0;
        let workspace_root = params.workspace_root;
        let document_path = params.document_path;
        let language_id = params.language_id.unwrap_or_else(|| "rust".to_owned());
        let text = params.text;
        let result = self
            .with_workspace_session_blocking(
                workspace_root,
                "open_document",
                move |session| {
                    let tracked = session.open_document(&document_path, language_id, 0, text)?;
                    Ok(OpenDocumentSummary {
                        document_path: tracked.path.clone(),
                        version: tracked.version(),
                    })
                },
            )
            .await?;

        Ok(Json(MutatingToolResult {
            data: result,
            execution: Default::default(),
            workspace_edit: None,
        }))
    }

    #[tool(
        name = "change_document",
        description = "Replace the full contents of an already-open document. The version must be strictly greater than the previous version.",
        annotations(
            title = "Change Document",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn change_document(
        &self,
        params: Parameters<ChangeDocumentInput>,
    ) -> Result<Json<MutatingToolResult<ChangeDocumentSummary>>, ErrorData> {
        let params = params.0;
        let workspace_root = params.workspace_root;
        let document_path = params.document_path;
        let version = params.version;
        let text = params.text;
        let result = self
            .with_workspace_session_blocking(
                workspace_root,
                "change_document",
                move |session| {
                    let tracked = session.change_document(&document_path, version, text)?;
                    Ok(ChangeDocumentSummary {
                        document_path: tracked.path.clone(),
                        version: tracked.version(),
                    })
                },
            )
            .await?;

        Ok(Json(MutatingToolResult {
            data: result,
            execution: Default::default(),
            workspace_edit: None,
        }))
    }

    #[tool(
        name = "close_document",
        description = "Stop synchronizing an open document with the workspace rust-analyzer session and release its tracked state.",
        annotations(
            title = "Close Document",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn close_document(
        &self,
        params: Parameters<DocumentInput>,
    ) -> Result<Json<MutatingToolResult<CloseDocumentSummary>>, ErrorData> {
        let params = params.0;
        let workspace_root = params.workspace_root;
        let document_path = params.document_path;
        let result = self
            .with_workspace_session_blocking(
                workspace_root,
                "close_document",
                move |session| {
                    let tracked = session.close_document(&document_path)?;
                    Ok(CloseDocumentSummary {
                        document_path: tracked.path,
                    })
                },
            )
            .await?;

        Ok(Json(MutatingToolResult {
            data: result,
            execution: Default::default(),
            workspace_edit: None,
        }))
    }

    #[tool(
        name = "rename_symbol",
        description = "Rename a symbol across the workspace and return the resulting workspace edits. The edits are reported but not automatically applied to disk.",
        annotations(
            title = "Rename Symbol",
            read_only_hint = false,
            destructive_hint = true,
            idempotent_hint = false,
            open_world_hint = false
        )
    )]
    async fn rename_symbol(
        &self,
        params: Parameters<RenameSymbolInput>,
    ) -> Result<Json<MutatingToolResult<RenameSummary>>, ErrorData> {
        let params = params.0;
        let workspace_root = params.workspace_root;
        let document_path = params.document_path;
        let position = to_lsp_position(params.position);
        let new_name = params.new_name;
        let (summary, edit) = self
            .with_workspace_session_blocking(
                workspace_root,
                "rename_symbol",
                move |session| {
                    let workspace_edit =
                        session.rename(&document_path, position, &new_name)?;
                    Ok((
                        RenameSummary {
                            new_name: new_name.clone(),
                        },
                        workspace_edit,
                    ))
                },
            )
            .await?;

        Ok(Json(MutatingToolResult {
            data: summary,
            execution: Default::default(),
            workspace_edit: edit.map(normalize_workspace_edit),
        }))
    }

    #[tool(
        name = "reload_workspace",
        description = "Ask rust-analyzer to reload the workspace configuration. Use this after Cargo.toml or project-level configuration changes.",
        annotations(
            title = "Reload Workspace",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn reload_workspace(
        &self,
        params: Parameters<WorkspaceRootInput>,
    ) -> Result<Json<MutatingToolResult<()>>, ErrorData> {
        let workspace_root = params.0.workspace_root;
        self.with_workspace_session_blocking(
            workspace_root,
            "reload_workspace",
            move |session| session.reload_workspace(),
        )
        .await?;

        Ok(Json(MutatingToolResult {
            data: (),
            execution: Default::default(),
            workspace_edit: None,
        }))
    }

    #[tool(
        name = "rebuild_proc_macros",
        description = "Trigger a rebuild of procedural macros for the workspace. Use this after changing proc-macro crate source.",
        annotations(
            title = "Rebuild Proc Macros",
            read_only_hint = false,
            destructive_hint = false,
            idempotent_hint = true,
            open_world_hint = false
        )
    )]
    async fn rebuild_proc_macros(
        &self,
        params: Parameters<WorkspaceRootInput>,
    ) -> Result<Json<MutatingToolResult<()>>, ErrorData> {
        let workspace_root = params.0.workspace_root;
        self.with_workspace_session_blocking(
            workspace_root,
            "rebuild_proc_macros",
            move |session| session.rebuild_proc_macros(),
        )
        .await?;

        Ok(Json(MutatingToolResult {
            data: (),
            execution: Default::default(),
            workspace_edit: None,
        }))
    }
}

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

#[cfg(test)]
mod tests {
    use super::{percent_decode_path, symbol_kind_name, uri_to_path};
    use crate::{SymbolKind, Uri};
    use std::path::PathBuf;
    use std::str::FromStr;

    #[test]
    fn file_uris_are_percent_decoded_before_path_conversion() {
        let uri = Uri::from_str("file:///tmp/workspace/src/with%20space.rs").expect("valid uri");
        assert_eq!(
            uri_to_path(&uri),
            PathBuf::from("/tmp/workspace/src/with space.rs")
        );
    }

    #[test]
    fn invalid_percent_sequences_are_preserved() {
        assert_eq!(
            percent_decode_path("/tmp/workspace/src/percent%2G.rs"),
            PathBuf::from("/tmp/workspace/src/percent%2G.rs")
        );
    }

    #[test]
    fn symbol_kind_names_are_stable_for_known_variants() {
        assert_eq!(symbol_kind_name(SymbolKind::FUNCTION), "function");
        assert_eq!(symbol_kind_name(SymbolKind::ENUM_MEMBER), "enum_member");
        assert_eq!(symbol_kind_name(SymbolKind::TYPE_PARAMETER), "type_parameter");
    }

    #[test]
    fn symbol_kind_names_fallback_for_unknown_variants() {
        let unknown = serde_json::from_value(serde_json::json!(99)).expect("unknown symbol kind");
        assert_eq!(symbol_kind_name(unknown), "unknown_99");
    }
}
