//! rust-analyzer-specific request surface kept separate from standard LSP types.

use super::{
    GotoDefinitionResponse, LocationLink, Position, TextDocumentIdentifier,
    TextDocumentPositionParams, Uri, WorkspaceSymbolResponse,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

macro_rules! ra_request {
    ($name:ident, $method:literal, $params:ty, $result:ty) => {
        #[derive(Debug)]
        pub enum $name {}

        impl super::request::Request for $name {
            type Params = $params;
            type Result = $result;

            const METHOD: &'static str = $method;
        }
    };
}

ra_request!(
    AnalyzerStatus,
    "rust-analyzer/analyzerStatus",
    AnalyzerStatusParams,
    String
);
ra_request!(
    FetchDependencyList,
    "rust-analyzer/fetchDependencyList",
    FetchDependencyListParams,
    FetchDependencyListResult
);
ra_request!(ReloadWorkspace, "rust-analyzer/reloadWorkspace", (), ());
ra_request!(RebuildProcMacros, "rust-analyzer/rebuildProcMacros", (), ());
ra_request!(
    ViewSyntaxTree,
    "rust-analyzer/viewSyntaxTree",
    ViewSyntaxTreeParams,
    String
);
ra_request!(
    ViewHir,
    "rust-analyzer/viewHir",
    TextDocumentPositionParams,
    String
);
ra_request!(
    ViewMir,
    "rust-analyzer/viewMir",
    TextDocumentPositionParams,
    String
);
ra_request!(
    ExpandMacro,
    "rust-analyzer/expandMacro",
    ExpandMacroParams,
    Option<ExpandedMacro>
);
ra_request!(
    Runnables,
    "experimental/runnables",
    RunnablesParams,
    Vec<Runnable>
);
ra_request!(
    RelatedTests,
    "rust-analyzer/relatedTests",
    TextDocumentPositionParams,
    Vec<TestInfo>
);
ra_request!(
    RustAnalyzerWorkspaceSymbol,
    "workspace/symbol",
    RustAnalyzerWorkspaceSymbolParams,
    Option<WorkspaceSymbolResponse>
);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AnalyzerStatusParams {
    pub text_document: Option<TextDocumentIdentifier>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchDependencyListParams {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrateInfoResult {
    pub name: Option<String>,
    pub version: Option<String>,
    pub path: Uri,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FetchDependencyListResult {
    pub crates: Vec<CrateInfoResult>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ViewSyntaxTreeParams {
    pub text_document: TextDocumentIdentifier,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpandMacroParams {
    pub text_document: TextDocumentIdentifier,
    pub position: Position,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExpandedMacro {
    pub name: String,
    pub expansion: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunnablesParams {
    pub text_document: TextDocumentIdentifier,
    pub position: Option<Position>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Runnable {
    pub label: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub location: Option<LocationLink>,
    pub kind: RunnableKind,
    pub args: RunnableArgs,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[serde(untagged)]
pub enum RunnableArgs {
    Cargo(CargoRunnableArgs),
    Shell(ShellRunnableArgs),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RunnableKind {
    Cargo,
    Shell,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CargoRunnableArgs {
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub environment: HashMap<String, String>,
    pub cwd: PathBuf,
    pub override_cargo: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub workspace_root: Option<PathBuf>,
    pub cargo_args: Vec<String>,
    pub executable_args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ShellRunnableArgs {
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub environment: HashMap<String, String>,
    pub cwd: PathBuf,
    pub program: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TestInfo {
    pub runnable: Runnable,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RustAnalyzerWorkspaceSymbolParams {
    #[serde(flatten)]
    pub partial_result_params: super::PartialResultParams,
    #[serde(flatten)]
    pub work_done_progress_params: super::WorkDoneProgressParams,
    pub query: String,
    pub search_scope: Option<WorkspaceSymbolSearchScope>,
    pub search_kind: Option<WorkspaceSymbolSearchKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkspaceSymbolSearchScope {
    Workspace,
    WorkspaceAndDependencies,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum WorkspaceSymbolSearchKind {
    OnlyTypes,
    AllSymbols,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenCargoTomlParams {
    pub text_document: TextDocumentIdentifier,
}

ra_request!(
    OpenCargoToml,
    "experimental/openCargoToml",
    OpenCargoTomlParams,
    Option<GotoDefinitionResponse>
);

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GetFailedObligationsParams {
    pub text_document: TextDocumentIdentifier,
    pub position: Position,
}

ra_request!(
    GetFailedObligations,
    "rust-analyzer/getFailedObligations",
    GetFailedObligationsParams,
    String
);
