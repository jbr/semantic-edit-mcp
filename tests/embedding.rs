//! Integration tests for the in-process embedding surface (`lib.rs`).

use mcplease::traits::Tool;
use semantic_edit_mcp::{SemanticEditTools, Tools};
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
#[test]
fn commit_fn_persists_across_multiple_edits() {
    let dir = std::env::temp_dir().join(format!("sem-edit-embed-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("demo.rs");
    std::fs::write(&file, "fn main() {\n    println!(\"a\");\n}\n").unwrap();

    let mut state = SemanticEditTools::embedded(dir.clone()).unwrap();

    let captured: Arc<Mutex<Vec<(PathBuf, String)>>> = Arc::new(Mutex::new(Vec::new()));
    let sink = captured.clone();
    state.set_commit_fn(Some(Box::new(move |path, content| {
        sink.lock().unwrap().push((path, content));
    })));

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
        .execute(&mut state)
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
#[test]
fn missing_file_is_an_error_not_a_panic() {
    let dir = std::env::temp_dir().join(format!("sem-edit-missing-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let mut state = SemanticEditTools::embedded(dir.clone()).unwrap();

    let result = tool(serde_json::json!({
        "name": "edit",
        "arguments": {
            "file_path": "does_not_exist.rs",
            "operation": "replace",
            "anchor": "fn main",
            "content": "fn main() {}",
        }
    }))
    .execute(&mut state);

    std::fs::remove_dir_all(&dir).ok();
    let err = result.expect_err("editing a missing file should be an error");
    assert!(
        err.to_string().contains("existing files"),
        "error should explain the file must exist, got: {err}"
    );
}
