use lsp_types::Position;
use rust_lsp_mcp::lsp_client::{
    ExpandMacro, ExpandedMacro, FetchDependencyListResult, RunnableArgs, RunnableKind,
    WorkspaceSessionBuilder,
};
use serde_json::json;
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn exposes_typed_rust_analyzer_extension_requests() -> Result<(), Box<dyn Error>> {
    let workspace_root = create_temp_workspace("ra-ext");
    let file_path = workspace_root.join("src").join("lib.rs");
    fs::create_dir_all(file_path.parent().expect("src dir"))?;
    fs::write(&file_path, "fn answer() -> u32 { 42 }\n")?;

    let mut session = spawn_workspace_session(&workspace_root)?;
    session.initialize()?;

    let analyzer_status = session.analyzer_status(Some(&file_path))?;
    assert!(analyzer_status.starts_with("status:file://"));

    let dependencies: FetchDependencyListResult = session.fetch_dependency_list()?;
    assert_eq!(dependencies.crates.len(), 1);
    assert_eq!(dependencies.crates[0].name.as_deref(), Some("serde"));

    let syntax_tree = session.view_syntax_tree(&file_path)?;
    assert!(syntax_tree.contains("syntax tree for file://"));

    let position = Position::new(0, 3);
    let hir = session.view_hir(&file_path, position)?;
    let mir = session.view_mir(&file_path, position)?;
    assert_eq!(hir, "hir debug output");
    assert_eq!(mir, "mir debug output");

    let expanded: Option<ExpandedMacro> = session.expand_macro(&file_path, position)?;
    assert_eq!(
        expanded,
        Some(ExpandedMacro {
            name: "println!".to_owned(),
            expansion: "std::io::_print(format_args!(\"hi\"))".to_owned(),
        })
    );

    let runnables = session.runnables(&file_path, Some(position))?;
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

    let related_tests = session.related_tests(&file_path, position)?;
    assert_eq!(related_tests.len(), 1);
    assert_eq!(related_tests[0].runnable.kind, RunnableKind::Cargo);

    session.reload_workspace()?;
    session.rebuild_proc_macros()?;

    let state = session.request("state", json!({}))?;
    assert_eq!(state["reload_workspace_requests"], 1);
    assert_eq!(state["rebuild_proc_macro_requests"], 1);

    let direct_expand: Option<ExpandedMacro> =
        session.request_typed::<ExpandMacro>(rust_lsp_mcp::lsp_client::ExpandMacroParams {
            text_document: lsp_types::TextDocumentIdentifier::new(
                session.workspace_uri().parse()?,
            ),
            position,
        })?;
    assert_eq!(direct_expand, expanded);

    session.shutdown()?;
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
