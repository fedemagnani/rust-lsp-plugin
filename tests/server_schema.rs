#![allow(missing_docs)]

use rmcp::model::ErrorCode;
use rust_lsp_mcp::{
    CancellationSummary, DocumentLocation, HoverContent, HoverSummary,
    MutatingToolResult, ReadOnlyToolResult, RenameSymbolInput, ServerError, ServerErrorKind,
    SessionError, TextEditSummary, TextPosition, TextRange, ToolExecutionSummary,
    ToolProgressPhase, ToolSemantics, WorkspaceChangeSummary, WorkspaceEditSummary,
    WorkspaceSessionError,
};
use schemars::schema_for;
use serde_json::json;
use std::path::PathBuf;
use std::time::Duration;

#[test]
fn tool_semantics_map_to_expected_rmcp_annotations() {
    let read_only = ToolSemantics::ReadOnly.annotations("Hover");
    assert_eq!(read_only.title.as_deref(), Some("Hover"));
    assert_eq!(read_only.read_only_hint, Some(true));
    assert_eq!(read_only.destructive_hint, Some(false));
    assert_eq!(read_only.idempotent_hint, Some(true));
    assert_eq!(read_only.open_world_hint, Some(false));

    let mutating = ToolSemantics::Mutating {
        destructive: false,
        idempotent: true,
    }
    .annotations("Rename");
    assert_eq!(mutating.read_only_hint, Some(false));
    assert_eq!(mutating.destructive_hint, Some(false));
    assert_eq!(mutating.idempotent_hint, Some(true));
    assert_eq!(mutating.open_world_hint, Some(false));
}

#[test]
fn representative_schema_inputs_remain_stable() {
    let schema = schema_for!(RenameSymbolInput);
    let schema_json = serde_json::to_value(&schema).expect("schema json");
    let properties = &schema_json["properties"];

    assert!(properties.get("workspace_root").is_some());
    assert!(properties.get("document_path").is_some());
    assert!(properties.get("position").is_some());
    assert!(properties.get("new_name").is_some());
}

#[test]
fn tool_results_serialize_without_transport_envelopes() {
    let read_only = ReadOnlyToolResult {
        data: HoverSummary {
            contents: HoverContent::Markdown("```rust\nfn answer() -> u32\n```".to_owned()),
            range: Some(TextRange {
                start: TextPosition {
                    line: 1,
                    character: 4,
                },
                end: TextPosition {
                    line: 1,
                    character: 10,
                },
            }),
        },
        execution: ToolExecutionSummary {
            progress_phase: ToolProgressPhase::Completed,
            progress_message: Some("workspace ready".to_owned()),
            cancellation: None,
        },
    };

    let mutating = MutatingToolResult {
        data: json!({ "renamed": true }),
        execution: ToolExecutionSummary {
            progress_phase: ToolProgressPhase::Completed,
            progress_message: None,
            cancellation: Some(CancellationSummary {
                requested: false,
                reason: None,
            }),
        },
        workspace_edit: Some(WorkspaceEditSummary {
            changes: vec![WorkspaceChangeSummary::TextEdits {
                document_path: PathBuf::from("/workspace/src/lib.rs"),
                edits: vec![TextEditSummary {
                    range: TextRange {
                        start: TextPosition {
                            line: 0,
                            character: 7,
                        },
                        end: TextPosition {
                            line: 0,
                            character: 13,
                        },
                    },
                    new_text: "renamed".to_owned(),
                }],
            }],
        }),
    };

    let read_only_json = serde_json::to_value(&read_only).expect("read only json");
    let mutating_json = serde_json::to_value(&mutating).expect("mutating json");

    assert!(read_only_json.get("jsonrpc").is_none());
    assert!(read_only_json.get("id").is_none());
    assert!(read_only_json.get("result").is_none());
    assert_eq!(read_only_json["execution"]["progress_phase"], json!("completed"));

    assert!(mutating_json.get("jsonrpc").is_none());
    assert_eq!(mutating_json["workspace_edit"]["changes"][0]["kind"], json!("text_edits"));
}

#[test]
fn server_error_maps_session_and_workspace_failures_into_stable_taxonomy() {
    let timeout = ServerError::from(SessionError::RequestTimeout {
        method: "textDocument/hover".to_owned(),
        timeout: Duration::from_millis(250),
    });
    assert_eq!(timeout.kind, ServerErrorKind::Timeout);
    assert!(timeout.retriable);
    let timeout_data = timeout.to_error_data();
    assert_eq!(timeout_data.code, ErrorCode(-32001));

    let invalid_phase = ServerError::from(WorkspaceSessionError::InvalidPhase {
        operation: "open_document",
        phase: rust_lsp_mcp::WorkspaceSessionPhase::PreInitialize,
    });
    assert_eq!(invalid_phase.kind, ServerErrorKind::NotReady);
    assert!(invalid_phase.retriable);

    let missing_document = ServerError::from(WorkspaceSessionError::DocumentNotOpen {
        path: PathBuf::from("/workspace/src/lib.rs"),
    });
    assert_eq!(missing_document.kind, ServerErrorKind::DocumentNotAvailable);
    let error_data = missing_document.to_error_data();
    assert_eq!(error_data.code, ErrorCode::RESOURCE_NOT_FOUND);
    assert_eq!(error_data.data.expect("details")["kind"], json!("document_not_available"));
}

#[test]
fn normalized_location_shape_stays_path_based() {
    let location = DocumentLocation {
        document_path: PathBuf::from("/workspace/src/lib.rs"),
        range: TextRange {
            start: TextPosition {
                line: 4,
                character: 8,
            },
            end: TextPosition {
                line: 4,
                character: 14,
            },
        },
    };

    let json = serde_json::to_value(location).expect("location json");
    assert_eq!(json["document_path"], json!("/workspace/src/lib.rs"));
    assert_eq!(json["range"]["start"]["line"], json!(4));
    assert_eq!(json["range"]["end"]["character"], json!(14));
}
