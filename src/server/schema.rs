//! MCP-facing input and output schemas shared across rust-analyzer tools.
//!
//! The shaping rules in this module are intentional:
//! - inputs use workspace/document/path primitives instead of raw JSON-RPC envelopes
//! - outputs use normalized summaries instead of raw LSP union payloads
//! - cancellation and progress surface through a shared execution summary
//! - mutating tools expose workspace-edit effects separately from the logical payload

use rmcp::model::ToolAnnotations;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Common behavior categories used when annotating MCP tools.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToolSemantics {
    /// The tool observes server state without mutating files or workspace state.
    ReadOnly,
    /// The tool mutates state or proposes edits.
    Mutating {
        /// Whether the mutation may remove or overwrite user state.
        destructive: bool,
        /// Whether repeating the same call leaves state unchanged after the first application.
        idempotent: bool,
    },
}

impl ToolSemantics {
    /// Builds rmcp tool annotations that match the declared semantics.
    pub fn annotations(self, title: impl Into<String>) -> ToolAnnotations {
        let title = title.into();
        match self {
            Self::ReadOnly => ToolAnnotations::with_title(title)
                .read_only(true)
                .destructive(false)
                .idempotent(true)
                .open_world(false),
            Self::Mutating {
                destructive,
                idempotent,
            } => ToolAnnotations::with_title(title)
                .read_only(false)
                .destructive(destructive)
                .idempotent(idempotent)
                .open_world(false),
        }
    }
}

/// Identifies a registered rust-analyzer workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkspaceRootInput {
    /// Absolute path to the workspace root managed by the server.
    pub workspace_root: PathBuf,
}

/// Identifies a specific document inside a registered workspace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DocumentInput {
    /// Absolute path to the workspace root managed by the server.
    pub workspace_root: PathBuf,
    /// Absolute path to the target document.
    pub document_path: PathBuf,
}

/// Zero-based text position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TextPosition {
    /// Zero-based line number.
    pub line: u32,
    /// Zero-based UTF-8 character offset on the line.
    pub character: u32,
}

/// Zero-based text range.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TextRange {
    /// Inclusive start position.
    pub start: TextPosition,
    /// Exclusive end position.
    pub end: TextPosition,
}

/// Identifies a position within a document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DocumentPositionInput {
    /// Absolute path to the workspace root managed by the server.
    pub workspace_root: PathBuf,
    /// Absolute path to the target document.
    pub document_path: PathBuf,
    /// Zero-based position within the target document.
    pub position: TextPosition,
}

/// Input shared by query-style workspace tools such as symbol search.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct WorkspaceQueryInput {
    /// Absolute path to the workspace root managed by the server.
    pub workspace_root: PathBuf,
    /// Search string interpreted by the downstream tool.
    pub query: String,
}

/// Representative mutating input for rename-style operations.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct RenameSymbolInput {
    /// Absolute path to the workspace root managed by the server.
    pub workspace_root: PathBuf,
    /// Absolute path to the target document.
    pub document_path: PathBuf,
    /// Zero-based position of the symbol being renamed.
    pub position: TextPosition,
    /// Replacement symbol name.
    pub new_name: String,
}

/// Normalized link to a location in source code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct DocumentLocation {
    /// Absolute path to the target document.
    pub document_path: PathBuf,
    /// Range inside the target document.
    pub range: TextRange,
}

/// Normalized hover content kinds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", content = "value", rename_all = "snake_case")]
pub enum HoverContent {
    /// Markdown-formatted hover body.
    Markdown(String),
    /// Plain text hover body.
    PlainText(String),
}

/// Representative read-only hover output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct HoverSummary {
    /// Human-meaningful hover contents.
    pub contents: HoverContent,
    /// Optional source range associated with the hover.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub range: Option<TextRange>,
}

/// Representative workspace symbol output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SymbolSummary {
    /// Symbol name as displayed to callers.
    pub name: String,
    /// Human-readable kind label.
    pub kind: String,
    /// Optional container name such as a module or type.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub container_name: Option<String>,
    /// Source location for the symbol.
    pub location: DocumentLocation,
}

/// Representative analyzer-status output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct AnalyzerStatusSummary {
    /// Human-readable analyzer status string.
    pub status: String,
}

/// Representative syntax-tree inspection output.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct SyntaxTreeSummary {
    /// Rendered syntax tree for the requested document.
    pub tree: String,
}

/// Most recent progress phase for a tool call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ToolProgressPhase {
    /// Work has been accepted but not started.
    Queued,
    /// Work is currently running.
    Running,
    /// Work completed normally.
    Completed,
}

/// Final cancellation state returned by a tool, when cancellation surfaced to the caller.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct CancellationSummary {
    /// Whether cancellation was requested while the tool was running.
    pub requested: bool,
    /// Optional reason or source for the cancellation.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reason: Option<String>,
}

/// Shared execution metadata carried by both read-only and mutating tool results.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ToolExecutionSummary {
    /// Most recent known progress phase for the tool call.
    pub progress_phase: ToolProgressPhase,
    /// Optional latest progress message.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub progress_message: Option<String>,
    /// Cancellation information when relevant to the call outcome.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cancellation: Option<CancellationSummary>,
}

impl Default for ToolExecutionSummary {
    fn default() -> Self {
        Self {
            progress_phase: ToolProgressPhase::Completed,
            progress_message: None,
            cancellation: None,
        }
    }
}

/// Summary of one text edit without leaking the full LSP workspace-edit envelope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct TextEditSummary {
    /// Affected range in the target document.
    pub range: TextRange,
    /// Replacement text.
    pub new_text: String,
}

/// Normalized summary of workspace edit effects produced by mutating tools.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkspaceChangeSummary {
    /// File text edits for an existing document.
    TextEdits {
        /// Absolute path to the edited document.
        document_path: PathBuf,
        /// Text edits that should be applied to the document.
        edits: Vec<TextEditSummary>,
    },
    /// File creation requested by the tool.
    CreateFile {
        /// Absolute path to the new file.
        path: PathBuf,
    },
    /// File rename requested by the tool.
    RenameFile {
        /// Existing file path.
        old_path: PathBuf,
        /// Replacement file path.
        new_path: PathBuf,
    },
    /// File deletion requested by the tool.
    DeleteFile {
        /// Absolute path to the deleted file.
        path: PathBuf,
    },
}

/// Shared workspace-edit summary used by mutating tools.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema, Default)]
pub struct WorkspaceEditSummary {
    /// Set of normalized file changes represented by the tool response.
    pub changes: Vec<WorkspaceChangeSummary>,
}

/// Shared wrapper for read-only tool responses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct ReadOnlyToolResult<T> {
    /// Normalized tool payload.
    pub data: T,
    /// Shared execution metadata such as progress and cancellation.
    #[serde(default)]
    pub execution: ToolExecutionSummary,
}

/// Shared wrapper for mutating tool responses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
pub struct MutatingToolResult<T> {
    /// Normalized tool payload.
    pub data: T,
    /// Shared execution metadata such as progress and cancellation.
    #[serde(default)]
    pub execution: ToolExecutionSummary,
    /// Normalized workspace edits associated with the mutation, when present.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_edit: Option<WorkspaceEditSummary>,
}
