use rust_lsp_mcp::{
    CompletionContext, CompletionResponse, CompletionTriggerKind, DocumentSymbolResponse,
    GotoDefinitionResponse, HoverContents, MarkupKind, OneOf, Position, PrepareRenameResponse,
    SymbolKind, WorkspaceSessionBuilder, WorkspaceSymbolResponse,
};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn exposes_typed_core_lsp_requests() -> Result<(), Box<dyn Error>> {
    let workspace_root = create_temp_workspace("core-lsp");
    let file_path = workspace_root.join("src").join("lib.rs");
    fs::create_dir_all(file_path.parent().expect("src dir"))?;
    fs::write(
        &file_path,
        "pub fn answer() -> u32 {\n    42\n}\n\npub fn use_answer() -> u32 {\n    answer()\n}\n",
    )?;

    let mut session = spawn_workspace_session(&workspace_root)?;
    session.initialize()?;
    session.open_document(&file_path, "rust", 1, fs::read_to_string(&file_path)?)?;

    let hover = session
        .hover(
            &file_path,
            Position {
                line: 5,
                character: 4,
            },
        )?
        .expect("hover result");
    match hover.contents {
        HoverContents::Markup(markup) => {
            assert_eq!(markup.kind, MarkupKind::Markdown);
            assert!(markup.value.contains("fn answer"));
        }
        other => panic!("unexpected hover contents: {other:?}"),
    }

    let completions = session
        .completion(
            &file_path,
            Position {
                line: 5,
                character: 4,
            },
            Some(CompletionContext {
                trigger_kind: CompletionTriggerKind::INVOKED,
                trigger_character: None,
            }),
        )?
        .expect("completion result");
    let completions = match completions {
        CompletionResponse::Array(items) => items,
        CompletionResponse::List(list) => {
            assert!(!list.is_incomplete);
            list.items
        }
    };
    assert_eq!(completions.len(), 1);
    assert_eq!(completions[0].label, "answer");

    let definitions = session.goto_definition(
        &file_path,
        Position {
            line: 5,
            character: 4,
        },
    )?;
    match definitions.expect("definition result") {
        GotoDefinitionResponse::Scalar(location) => {
            assert!(location.uri.as_str().ends_with("/src/lib.rs"));
            assert_eq!(location.range.start.line, 1);
        }
        GotoDefinitionResponse::Array(locations) => {
            assert_eq!(locations.len(), 1);
            assert!(locations[0].uri.as_str().ends_with("/src/lib.rs"));
        }
        other => panic!("unexpected definition target: {other:?}"),
    }

    let references = session.references(
        &file_path,
        Position {
            line: 5,
            character: 4,
        },
        true,
    )?;
    let references = references.expect("references result");
    assert_eq!(references.len(), 2);
    assert!(
        references
            .iter()
            .all(|reference| reference.uri.as_str().ends_with("/src/lib.rs"))
    );

    let rename = session
        .prepare_rename(
            &file_path,
            Position {
                line: 5,
                character: 4,
            },
        )?
        .expect("prepare rename result");
    match rename {
        PrepareRenameResponse::RangeWithPlaceholder { range, placeholder } => {
            assert_eq!(range.start.line, 1);
            assert_eq!(placeholder, "answer");
        }
        other => panic!("unexpected prepare rename result: {other:?}"),
    }

    let edit = session
        .rename(
            &file_path,
            Position {
                line: 5,
                character: 4,
            },
            "meaning",
        )?
        .expect("rename edit");
    let changes = edit.changes.expect("rename changes");
    let file_uri = session
        .document(&file_path)?
        .expect("tracked document")
        .uri()
        .clone();
    let edits = changes.get(&file_uri).expect("edits for file");
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].new_text, "meaning");

    let document_symbols = session
        .document_symbols(&file_path)?
        .expect("document symbols");
    match document_symbols {
        DocumentSymbolResponse::Nested(symbols) => {
            assert_eq!(symbols.len(), 1);
            let symbol = &symbols[0];
            assert_eq!(symbol.name, "answer");
            assert_eq!(symbol.kind, SymbolKind::FUNCTION);
        }
        other => panic!("unexpected document symbol: {other:?}"),
    }

    let workspace_symbols = session
        .workspace_symbols("answer")?
        .expect("workspace symbols");
    match workspace_symbols {
        WorkspaceSymbolResponse::Nested(symbols) => {
            assert_eq!(symbols.len(), 1);
            let symbol = &symbols[0];
            assert_eq!(symbol.name, "answer");
            assert_eq!(symbol.container_name.as_deref(), Some("crate"));
            match &symbol.location {
                OneOf::Left(location) => assert!(location.uri.as_str().contains("/workspace/")),
                other => panic!("unexpected workspace symbol location: {other:?}"),
            }
        }
        WorkspaceSymbolResponse::Flat(symbols) => {
            assert_eq!(symbols.len(), 1);
            let symbol = &symbols[0];
            assert_eq!(symbol.name, "answer");
            assert_eq!(symbol.kind, SymbolKind::FUNCTION);
            assert_eq!(symbol.container_name.as_deref(), Some("crate"));
            assert!(symbol.location.uri.as_str().contains("/workspace/"));
        }
    }

    session.shutdown()?;
    remove_temp_workspace(&workspace_root);
    Ok(())
}

fn spawn_workspace_session(
    workspace_root: &Path,
) -> Result<rust_lsp_mcp::WorkspaceSession, Box<dyn Error>> {
    let program = std::env::var("CARGO_BIN_EXE_mock_rust_analyzer")?;
    Ok(WorkspaceSessionBuilder::new(program, workspace_root).spawn()?)
}

fn create_temp_workspace(label: &str) -> PathBuf {
    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("time went backwards")
        .as_nanos();
    let path = std::env::temp_dir().join(format!(
        "rust-lsp-mcp-{label}-{}-{unique}",
        std::process::id()
    ));
    fs::create_dir_all(&path).expect("create temp workspace");
    path
}

fn remove_temp_workspace(path: &Path) {
    let _ = fs::remove_dir_all(path);
}
