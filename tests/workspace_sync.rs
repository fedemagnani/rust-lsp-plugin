use rust_lsp_mcp::{
    WatchedFileChange, WatchedFileChangeKind, WorkspaceSessionBuilder, WorkspaceSessionError,
};
use serde_json::{Value, json};
use std::error::Error;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn synchronizes_open_change_save_and_close() -> Result<(), Box<dyn Error>> {
    let workspace_root = create_temp_workspace("sync-doc");
    let file_path = workspace_root.join("src").join("lib.rs");
    fs::create_dir_all(file_path.parent().expect("src dir"))?;
    fs::write(&file_path, "fn main() {}\n")?;

    let mut session = spawn_workspace_session(&workspace_root)?;
    session.initialize()?;

    let opened = session.open_document(&file_path, "rust", 1, "fn main() {}\n")?;
    assert_eq!(opened.version, 1);
    assert_eq!(opened.text, "fn main() {}\n");
    assert_eq!(session.open_documents()?.len(), 1);

    let changed = session.change_document(&file_path, 2, "fn main() { println!(\"hi\"); }\n")?;
    assert_eq!(changed.version, 2);
    assert_eq!(changed.text, "fn main() { println!(\"hi\"); }\n");
    session.save_document(&file_path)?;

    let uri = session
        .document(&file_path)?
        .expect("tracked document after save")
        .uri
        .clone();
    let uri_text = uri.as_str().to_owned();
    let state = session.request("state", json!({}))?;
    let server_document = state["open_documents"][&uri_text].clone();
    assert_eq!(server_document["languageId"], "rust");
    assert_eq!(server_document["version"], 2);
    assert_eq!(server_document["text"], "fn main() { println!(\"hi\"); }\n");
    assert!(
        state["notifications"]
            .as_array()
            .expect("notifications array")
            .iter()
            .any(|value| value == "textDocument/didSave")
    );

    let closed = session.close_document(&file_path)?;
    assert_eq!(closed.uri, uri);
    assert!(session.document(&file_path)?.is_none());

    let state = session.request("state", json!({}))?;
    assert!(state["open_documents"].get(&uri_text).is_none());
    assert!(
        state["closed_documents"]
            .as_array()
            .expect("closed documents")
            .iter()
            .any(|value| value == &Value::String(uri_text.clone()))
    );

    session.shutdown()?;
    remove_temp_workspace(&workspace_root);
    Ok(())
}

#[test]
fn forwards_workspace_configuration_and_watched_file_changes() -> Result<(), Box<dyn Error>> {
    let workspace_root = create_temp_workspace("sync-workspace");
    let cargo_toml = workspace_root.join("Cargo.toml");
    fs::write(
        &cargo_toml,
        "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\n",
    )?;

    let mut session = spawn_workspace_session(&workspace_root)?;
    session.initialize()?;

    session.change_configuration(json!({
        "rust-analyzer": {
            "checkOnSave": false
        }
    }))?;
    session.change_watched_files([WatchedFileChange {
        path: cargo_toml.clone(),
        kind: WatchedFileChangeKind::Changed,
    }])?;

    let state = session.request("state", json!({}))?;
    assert_eq!(
        state["configuration_changes"][0]["settings"]["rust-analyzer"]["checkOnSave"],
        false
    );
    assert_eq!(state["watched_file_changes"][0]["type"], 2);
    assert_eq!(
        state["watched_file_changes"][0]["uri"],
        Value::String(path_to_file_uri(&fs::canonicalize(&cargo_toml)?))
    );

    session.shutdown()?;
    remove_temp_workspace(&workspace_root);
    Ok(())
}

#[test]
fn rejects_non_monotonic_document_versions() -> Result<(), Box<dyn Error>> {
    let workspace_root = create_temp_workspace("sync-versions");
    let file_path = workspace_root.join("main.rs");
    fs::write(&file_path, "fn main() {}\n")?;

    let mut session = spawn_workspace_session(&workspace_root)?;
    session.initialize()?;
    session.open_document(&file_path, "rust", 5, "fn main() {}\n")?;

    let error = session
        .change_document(&file_path, 5, "fn main() { unreachable!(); }\n")
        .expect_err("non-monotonic version must fail");
    match error {
        WorkspaceSessionError::NonMonotonicDocumentVersion {
            current_version,
            new_version,
            ..
        } => {
            assert_eq!(current_version, 5);
            assert_eq!(new_version, 5);
        }
        other => panic!("unexpected error: {other}"),
    }

    session.shutdown()?;
    remove_temp_workspace(&workspace_root);
    Ok(())
}

#[test]
fn document_queries_require_ready_phase() -> Result<(), Box<dyn Error>> {
    let workspace_root = create_temp_workspace("sync-phase");
    let file_path = workspace_root.join("main.rs");

    let session = spawn_workspace_session(&workspace_root)?;

    let document_error = session
        .document(&file_path)
        .expect_err("document lookup before initialize must fail");
    assert!(matches!(
        document_error,
        WorkspaceSessionError::InvalidPhase {
            operation: "document",
            ..
        }
    ));

    match session.open_documents() {
        Err(WorkspaceSessionError::InvalidPhase {
            operation: "open_documents",
            ..
        }) => {}
        Err(other) => panic!("unexpected error: {other}"),
        Ok(_) => panic!("document listing before initialize must fail"),
    }

    remove_temp_workspace(&workspace_root);
    Ok(())
}

#[test]
fn normalizes_dot_segments_for_document_tracking() -> Result<(), Box<dyn Error>> {
    let workspace_root = create_temp_workspace("sync-normalize");
    let file_path = workspace_root.join("src").join("lib.rs");
    fs::create_dir_all(file_path.parent().expect("src dir"))?;
    fs::write(&file_path, "fn main() {}\n")?;

    let aliased_path = workspace_root
        .join("src")
        .join(".")
        .join("nested")
        .join("..")
        .join("lib.rs");

    let mut session = spawn_workspace_session(&workspace_root)?;
    session.initialize()?;
    session.open_document(&aliased_path, "rust", 1, "fn main() {}\n")?;
    session.change_document(&file_path, 2, "fn main() { println!(\"alias\"); }\n")?;

    let tracked = session
        .document(&file_path)?
        .expect("tracked document through canonical path");
    assert_eq!(tracked.version, 2);
    assert_eq!(tracked.path, fs::canonicalize(&file_path)?);

    session.shutdown()?;
    remove_temp_workspace(&workspace_root);
    Ok(())
}

#[test]
#[cfg(unix)]
fn resolves_symlinked_document_paths_to_the_same_entry() -> Result<(), Box<dyn Error>> {
    use std::os::unix::fs::symlink;

    let workspace_root = create_temp_workspace("sync-symlink");
    let real_dir = workspace_root.join("real");
    let link_dir = workspace_root.join("link");
    let real_path = real_dir.join("lib.rs");
    fs::create_dir_all(&real_dir)?;
    fs::write(&real_path, "fn main() {}\n")?;
    symlink(&real_dir, &link_dir)?;

    let symlink_path = link_dir.join("lib.rs");

    let mut session = spawn_workspace_session(&workspace_root)?;
    session.initialize()?;
    session.open_document(&symlink_path, "rust", 1, "fn main() {}\n")?;
    session.change_document(&real_path, 2, "fn main() { println!(\"symlink\"); }\n")?;

    let tracked = session
        .document(&real_path)?
        .expect("tracked document through real path");
    assert_eq!(tracked.version, 2);
    assert_eq!(tracked.path, fs::canonicalize(&real_path)?);

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

fn path_to_file_uri(path: &Path) -> String {
    let mut uri = String::from("file://");

    for component in path.components() {
        match component {
            std::path::Component::RootDir => uri.push('/'),
            std::path::Component::Normal(segment) => {
                if !uri.ends_with('/') {
                    uri.push('/');
                }
                uri.push_str(&percent_encode(segment));
            }
            std::path::Component::CurDir => {}
            std::path::Component::ParentDir => {
                if !uri.ends_with('/') {
                    uri.push('/');
                }
                uri.push_str("..");
            }
            std::path::Component::Prefix(prefix) => {
                uri.push('/');
                uri.push_str(&percent_encode(prefix.as_os_str()));
            }
        }
    }

    uri
}

fn percent_encode(segment: &std::ffi::OsStr) -> String {
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
