use rust_lsp_mcp::{
    CompletionContext, DefinitionTarget, DocumentSymbolItem, HoverContents, MarkupKind, Position,
    PrepareRenameResponse, WorkspaceSessionBuilder, WorkspaceSymbolItem,
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
                trigger_kind: 1,
                trigger_character: None,
            }),
        )?
        .expect("completion result");
    assert!(!completions.is_incomplete);
    assert_eq!(completions.items.len(), 1);
    assert_eq!(completions.items[0].label, "answer");

    let definitions = session.goto_definition(
        &file_path,
        Position {
            line: 5,
            character: 4,
        },
    )?;
    assert_eq!(definitions.len(), 1);
    match &definitions[0] {
        DefinitionTarget::Location(location) => {
            assert!(location.uri.ends_with("/src/lib.rs"));
            assert_eq!(location.range.start.line, 1);
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
    assert_eq!(references.len(), 2);
    assert!(
        references
            .iter()
            .all(|reference| reference.uri.ends_with("/src/lib.rs"))
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

    let edit = session.rename(
        &file_path,
        Position {
            line: 5,
            character: 4,
        },
        "meaning",
    )?;
    let changes = edit.changes.expect("rename changes");
    let file_uri = session
        .document(&file_path)?
        .expect("tracked document")
        .uri
        .clone();
    let edits = changes.get(&file_uri).expect("edits for file");
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].new_text, "meaning");

    let document_symbols = session.document_symbols(&file_path)?;
    assert_eq!(document_symbols.len(), 1);
    match &document_symbols[0] {
        DocumentSymbolItem::DocumentSymbol(symbol) => {
            assert_eq!(symbol.name, "answer");
            assert_eq!(symbol.kind, 12);
        }
        other => panic!("unexpected document symbol: {other:?}"),
    }

    let workspace_symbols = session.workspace_symbols("answer")?;
    assert_eq!(workspace_symbols.len(), 1);
    match &workspace_symbols[0] {
        WorkspaceSymbolItem::WorkspaceSymbol(symbol) => {
            assert_eq!(symbol.name, "answer");
            assert_eq!(symbol.container_name.as_deref(), Some("crate"));
        }
        other => panic!("unexpected workspace symbol: {other:?}"),
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
