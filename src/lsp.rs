//! Typed LSP request and response structures exposed by the public client API.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A zero-based LSP text document position.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Position {
    /// Zero-based line offset.
    pub line: u32,
    /// Zero-based UTF code unit offset within the line.
    pub character: u32,
}

/// An LSP text range.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Range {
    /// Range start position.
    pub start: Position,
    /// Range end position.
    pub end: Position,
}

/// A location inside a text document.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    /// Document URI.
    pub uri: String,
    /// Range inside the document.
    pub range: Range,
}

/// A location link returned by navigation requests.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LocationLink {
    /// Origin selection range, when provided by the server.
    pub origin_selection_range: Option<Range>,
    /// Target document URI.
    pub target_uri: String,
    /// Full target range.
    pub target_range: Range,
    /// Target selection range.
    pub target_selection_range: Range,
}

/// A navigation result item normalized from definition responses.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum DefinitionTarget {
    /// A plain document location.
    Location(Location),
    /// A richer location link.
    LocationLink(LocationLink),
}

/// A string with an associated language identifier.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LanguageString {
    /// Language identifier such as `rust`.
    pub language: String,
    /// Marked string value.
    pub value: String,
}

/// Rich markup content.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarkupContent {
    /// Markup kind.
    pub kind: MarkupKind,
    /// Markup value.
    pub value: String,
}

/// Supported markup kinds for hover and documentation payloads.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarkupKind {
    /// Plain text markup.
    #[serde(rename = "plaintext")]
    PlainText,
    /// Markdown markup.
    #[serde(rename = "markdown")]
    Markdown,
}

/// Hover or documentation contents.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum HoverContents {
    /// A single plain string.
    Scalar(String),
    /// A language-tagged string.
    LanguageString(LanguageString),
    /// Multiple marked strings.
    Array(Vec<MarkedString>),
    /// Structured markup content.
    Markup(MarkupContent),
}

/// A single marked string value.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum MarkedString {
    /// Plain text string.
    Scalar(String),
    /// A language-tagged string.
    LanguageString(LanguageString),
}

/// Hover result for a document position.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Hover {
    /// Hover contents.
    pub contents: HoverContents,
    /// Range associated with the hover, when present.
    pub range: Option<Range>,
}

/// Additional completion request context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionContext {
    /// Why completion was triggered.
    pub trigger_kind: u8,
    /// Trigger character when the request was character-triggered.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_character: Option<String>,
}

/// Completion item documentation payload.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum Documentation {
    /// Plain string documentation.
    Scalar(String),
    /// Structured markup documentation.
    Markup(MarkupContent),
}

/// A completion item returned by the server.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CompletionItem {
    /// Display label.
    pub label: String,
    /// Optional kind numeric discriminator.
    pub kind: Option<u8>,
    /// Optional detail string.
    pub detail: Option<String>,
    /// Optional documentation.
    pub documentation: Option<Documentation>,
    /// Optional text inserted on completion acceptance.
    pub insert_text: Option<String>,
}

/// Normalized completion result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletionItems {
    /// Whether the list is incomplete and may need retriggering.
    pub is_incomplete: bool,
    /// Completion items.
    pub items: Vec<CompletionItem>,
}

/// A document text edit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TextEdit {
    /// Edited range.
    pub range: Range,
    /// Replacement text.
    pub new_text: String,
}

/// Workspace edit returned by rename-like requests.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceEdit {
    /// URI-keyed edits grouped by document.
    pub changes: Option<HashMap<String, Vec<TextEdit>>>,
}

/// Result of `textDocument/prepareRename`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum PrepareRenameResponse {
    /// The server returned only the editable range.
    Range(Range),
    /// The server returned a range and placeholder.
    RangeWithPlaceholder {
        /// Editable range.
        range: Range,
        /// Existing symbol text.
        placeholder: String,
    },
    /// The server delegated to default client behavior.
    #[serde(rename_all = "camelCase")]
    DefaultBehavior {
        /// Default behavior flag.
        default_behavior: bool,
    },
}

/// A flat symbol information entry.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolInformation {
    /// Symbol name.
    pub name: String,
    /// Symbol kind numeric discriminator.
    pub kind: u8,
    /// Symbol location.
    pub location: Location,
    /// Optional container name.
    pub container_name: Option<String>,
}

/// A hierarchical document symbol.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DocumentSymbol {
    /// Symbol name.
    pub name: String,
    /// Optional symbol detail.
    pub detail: Option<String>,
    /// Symbol kind numeric discriminator.
    pub kind: u8,
    /// Full symbol range.
    pub range: Range,
    /// Preferred selection range.
    pub selection_range: Range,
    /// Child document symbols.
    #[serde(default)]
    pub children: Vec<DocumentSymbol>,
}

/// A normalized document symbol result entry.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum DocumentSymbolItem {
    /// Hierarchical document symbol.
    DocumentSymbol(DocumentSymbol),
    /// Flat symbol information.
    SymbolInformation(SymbolInformation),
}

/// A workspace symbol entry.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSymbol {
    /// Symbol name.
    pub name: String,
    /// Symbol kind numeric discriminator.
    pub kind: u8,
    /// Symbol location.
    pub location: WorkspaceSymbolLocation,
    /// Optional container name.
    pub container_name: Option<String>,
}

/// Workspace symbol location variants.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum WorkspaceSymbolLocation {
    /// Resolved concrete location.
    Location(Location),
    /// Unresolved URI without a range.
    Uri { uri: String },
}

/// A normalized workspace symbol result entry.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub enum WorkspaceSymbolItem {
    /// Rich workspace symbol result.
    WorkspaceSymbol(WorkspaceSymbol),
    /// Legacy symbol information result.
    SymbolInformation(SymbolInformation),
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub(crate) enum CompletionResponse {
    Array(Vec<CompletionItem>),
    List(CompletionList),
}

impl CompletionResponse {
    pub(crate) fn into_completion_items(self) -> CompletionItems {
        match self {
            Self::Array(items) => CompletionItems {
                is_incomplete: false,
                items,
            },
            Self::List(list) => CompletionItems {
                is_incomplete: list.is_incomplete,
                items: list.items,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct CompletionList {
    pub(crate) is_incomplete: bool,
    pub(crate) items: Vec<CompletionItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(untagged)]
pub(crate) enum DefinitionResponse {
    Scalar(Location),
    Locations(Vec<Location>),
    LocationLinks(Vec<LocationLink>),
}

impl DefinitionResponse {
    pub(crate) fn into_targets(self) -> Vec<DefinitionTarget> {
        match self {
            Self::Scalar(location) => vec![DefinitionTarget::Location(location)],
            Self::Locations(locations) => locations
                .into_iter()
                .map(DefinitionTarget::Location)
                .collect(),
            Self::LocationLinks(links) => links
                .into_iter()
                .map(DefinitionTarget::LocationLink)
                .collect(),
        }
    }
}
