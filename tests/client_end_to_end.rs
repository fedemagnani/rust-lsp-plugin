use lsp_types::{
    GotoDefinitionResponse, HoverContents, MarkupKind, Position, WorkspaceSymbolResponse,
};
use rust_lsp_mcp::lsp_client::{
    ExpandedMacro, RunnableArgs, RunnableKind, WorkspaceLoadingState, WorkspaceSessionBuilder,
    WorkspaceSessionPhase,
};
use serde_json::json;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn workspace_session_supports_an_end_to_end_client_flow() -> Result<(), Box<dyn Error>> {
    let workspace_root = create_temp_workspace("end-to-end");
    let file_path = workspace_root.join("src").join("lib.rs");
    fs::create_dir_all(file_path.parent().expect("src dir"))?;

    let initial_text = "pub fn answer() -> u32 {\n    42\n}\n";
    let synced_text = concat!(
        "pub fn answer() -> u32 {\n",
        "    42\n",
        "}\n",
        "\n",
        "pub fn use_answer() -> u32 {\n",
        "    println!(\"{}\", answer());\n",
        "    answer()\n",
        "}\n",
    );
    fs::write(&file_path, initial_text)?;

    let mut session = spawn_workspace_session(&workspace_root)?;

    assert_eq!(session.phase(), WorkspaceSessionPhase::PreInitialize);
    let ready = session.initialize()?.clone();
    assert_eq!(session.phase(), WorkspaceSessionPhase::Ready);
    assert!(ready.configuration_requested);
    assert_eq!(ready.loading_state, WorkspaceLoadingState::Ready);
    assert_eq!(session.loading_state(), &WorkspaceLoadingState::Ready);

    let opened = session.open_document(&file_path, "rust", 1, initial_text)?;
    assert_eq!(opened.version(), 1);
    assert_eq!(opened.text(), initial_text);

    let changed = session.change_document(&file_path, 2, synced_text)?;
    assert_eq!(changed.version(), 2);
    assert_eq!(changed.text(), synced_text);
    session.save_document(&file_path)?;

    let tracked = session
        .document(&file_path)?
        .expect("tracked document after save");
    assert_eq!(tracked.version(), 2);
    assert_eq!(tracked.text(), synced_text);
    assert_eq!(session.open_documents()?.len(), 1);

    let state = session.request("state", json!({}))?;
    let file_uri = tracked.uri().as_str().to_owned();
    assert_eq!(state["open_documents"][&file_uri]["version"], 2);
    assert_eq!(state["open_documents"][&file_uri]["text"], synced_text);
    assert!(
        state["notifications"]
            .as_array()
            .expect("notifications array")
            .iter()
            .any(|value| value == "textDocument/didOpen")
    );
    assert!(
        state["notifications"]
            .as_array()
            .expect("notifications array")
            .iter()
            .any(|value| value == "textDocument/didChange")
    );
    assert!(
        state["notifications"]
            .as_array()
            .expect("notifications array")
            .iter()
            .any(|value| value == "textDocument/didSave")
    );

    let call_site = Position::new(6, 4);
    let hover = session
        .hover(&file_path, call_site)?
        .expect("hover result for synchronized document");
    match hover.contents {
        HoverContents::Markup(markup) => {
            assert_eq!(markup.kind, MarkupKind::Markdown);
            assert!(markup.value.contains("fn answer"));
        }
        other => panic!("unexpected hover contents: {other:?}"),
    }

    let definitions = session
        .goto_definition(&file_path, call_site)?
        .expect("definition result");
    let definition_uri = match definitions {
        GotoDefinitionResponse::Scalar(location) => location.uri,
        GotoDefinitionResponse::Array(locations) => {
            assert_eq!(locations.len(), 1);
            locations[0].uri.clone()
        }
        other => panic!("unexpected definition result: {other:?}"),
    };
    assert!(definition_uri.as_str().ends_with("/src/lib.rs"));

    let references = session
        .references(&file_path, call_site, true)?
        .expect("references result");
    assert_eq!(references.len(), 2);

    let symbols = session
        .workspace_symbols("answer")?
        .expect("workspace symbols");
    match symbols {
        WorkspaceSymbolResponse::Nested(symbols) => {
            assert_eq!(symbols.len(), 1);
            assert_eq!(symbols[0].name, "answer");
        }
        WorkspaceSymbolResponse::Flat(symbols) => {
            assert_eq!(symbols.len(), 1);
            assert_eq!(symbols[0].name, "answer");
        }
    }

    let analyzer_status = session.analyzer_status(Some(&file_path))?;
    assert!(analyzer_status.starts_with("status:file://"));

    let syntax_tree = session.view_syntax_tree(&file_path)?;
    assert!(syntax_tree.contains("syntax tree for file://"));

    let macro_position = Position::new(5, 4);
    let expanded = session.expand_macro(&file_path, macro_position)?;
    assert_eq!(
        expanded,
        Some(ExpandedMacro {
            name: "println!".to_owned(),
            expansion: "std::io::_print(format_args!(\"hi\"))".to_owned(),
        })
    );

    let runnables = session.runnables(&file_path, Some(macro_position))?;
    assert_eq!(runnables.len(), 1);
    assert_eq!(runnables[0].kind, RunnableKind::Cargo);
    match &runnables[0].args {
        RunnableArgs::Cargo(args) => {
            assert_eq!(args.cargo_args, vec!["test", "answer"]);
            assert_eq!(
                args.workspace_root.as_deref(),
                Some(Path::new("/workspace"))
            );
        }
        other => panic!("unexpected runnable args: {other:?}"),
    }

    let closed = session.close_document(&file_path)?;
    assert_eq!(closed.version(), 2);
    assert!(session.document(&file_path)?.is_none());

    session.shutdown()?;
    assert_eq!(session.phase(), WorkspaceSessionPhase::Shutdown);

    remove_temp_workspace(&workspace_root);
    Ok(())
}

fn spawn_workspace_session(
    workspace_root: &Path,
) -> Result<rust_lsp_mcp::lsp_client::WorkspaceSession, Box<dyn Error>> {
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
