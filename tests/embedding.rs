//! Integration tests for the in-process embedding surface (`lib.rs`).

use mcplease::traits::Dispatch;
use mcplease::types::RequestContext;
use semantic_edit_mcp::{Formatter, Formatting, LanguageName, SemanticEditTools, Tools};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
};

fn tool(json: serde_json::Value) -> Tools {
    serde_json::from_value(json).expect("valid tool call")
}

/// A `commit_fn` set once must be invoked for *every* successful `edit`, not just
/// the first. (Regression: the persist path used to `.take()` the closure, so the
/// second and later persists silently bypassed the hook and wrote to disk instead.)
#[tokio::test]
async fn commit_fn_persists_across_multiple_edits() {
    let dir = std::env::temp_dir().join(format!("sem-edit-embed-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("demo.rs");
    std::fs::write(&file, "fn main() {\n    println!(\"a\");\n}\n").unwrap();

    let captured: Arc<Mutex<Vec<(PathBuf, String)>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = captured.clone();
    let state = SemanticEditTools::new(dir.clone())
        .unwrap()
        .with_commit_fn(Arc::new(move |path, content| {
            sink.lock().unwrap().push((path, content));
        }));

    for content in ["\n    println!(\"b\");", "\n    println!(\"c\");"] {
        tool(serde_json::json!({
            "name": "edit",
            "arguments": {
                "file_path": "demo.rs",
                "operation": "insert_after",
                "anchor": "fn main() {",
                "content": content,
            }
        }))
        .call_to_text(state.clone(), RequestContext::default())
        .await
        .unwrap();
    }

    let count = captured.lock().unwrap().len();
    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(
        count, 2,
        "commit_fn should be invoked once per successful edit, got {count}"
    );
}

/// Editing a file that doesn't exist must return a clean `Err` (with an actionable
/// message), not panic — a panic in the embedded path would take down the host.
#[tokio::test]
async fn missing_file_is_an_error_not_a_panic() {
    let dir = std::env::temp_dir().join(format!("sem-edit-missing-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let state = SemanticEditTools::new(dir.clone()).unwrap();

    let result = tool(serde_json::json!({
        "name": "edit",
        "arguments": {
            "file_path": "does_not_exist.rs",
            "operation": "replace",
            "anchor": "fn main",
            "content": "fn main() {}",
        }
    }))
    .call_to_text(state.clone(), RequestContext::default())
    .await;

    std::fs::remove_dir_all(&dir).ok();
    let err = result.expect_err("editing a missing file should be an error");
    assert!(
        err.to_string().contains("existing files"),
        "error should explain the file must exist, got: {err}"
    );
}

/// Formatting is configured when the tools are built and can be replaced while
/// they run: each edit uses the settings current when it starts.
#[tokio::test]
async fn formatting_settings_reach_later_edits() {
    let dir = std::env::temp_dir().join(format!("sem-edit-formatting-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("demo.rs");
    std::fs::write(&file, "fn main() {\n    let a = 1;\n}\n").unwrap();

    let state = SemanticEditTools::new(dir.clone())
        .unwrap()
        .with_formatting(Formatting::off());
    // The handle an embedder keeps after giving a clone to its server.
    let handle = state.clone();
    let replace = |anchor: &str, content: &str| {
        tool(serde_json::json!({
            "name": "edit",
            "arguments": {
                "file_path": "demo.rs",
                "operation": "replace",
                "anchor": anchor,
                "content": content,
            }
        }))
    };

    replace("let a = 1;", "let a=2;")
        .call_to_text(state.clone(), RequestContext::default())
        .await
        .unwrap();
    let unformatted = std::fs::read_to_string(&file).unwrap();

    handle.set_formatting(Formatting::off().with(LanguageName::Rust, Formatter::rustfmt()));
    replace("let a=2;", "let a=3;")
        .call_to_text(state.clone(), RequestContext::default())
        .await
        .unwrap();
    let formatted = std::fs::read_to_string(&file).unwrap();

    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(unformatted, "fn main() {\n    let a=2;\n}\n");
    assert_eq!(formatted, "fn main() {\n    let a = 3;\n}\n");
}
