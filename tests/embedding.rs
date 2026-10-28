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

/// Tools built without formatting settings run no formatter: an edit is written
/// as laid out, at the indentation of the code it replaces, with its own spacing
/// kept and nothing else in the file touched.
#[tokio::test]
async fn by_default_an_edit_is_written_as_laid_out() {
    let dir = std::env::temp_dir().join(format!("sem-edit-unformatted-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("demo.rs");
    let original = "struct S;\n\nimpl S {\n    fn a(&self) -> u8 {\n        1\n    }\n}\n\nfn  untouched( ) {}\n";
    std::fs::write(&file, original).unwrap();

    let state = SemanticEditTools::new(dir.clone()).unwrap();
    assert_eq!(*state.formatting(), Formatting::off());

    let response = tool(serde_json::json!({
        "name": "edit",
        "arguments": {
            "file_path": "demo.rs",
            "operation": "replace",
            "anchor": "fn a(&self) -> u8 {\n        1\n    }",
            // A bare first line, and continuation lines at the file's columns.
            "content": "fn a(&self) -> u8 {\n        let x=2;\n        x\n    }",
        }
    }))
    .call_to_text(state.clone(), RequestContext::default())
    .await
    .unwrap();
    let edited = std::fs::read_to_string(&file).unwrap();

    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(
        edited,
        "struct S;\n\nimpl S {\n    fn a(&self) -> u8 {\n        let x=2;\n        x\n    }\n}\n\nfn  untouched( ) {}\n"
    );
    assert!(response.contains("no formatter is run"), "{response}");
}

/// [`Formatting::built_in`] is each language's built-in formatter: taplo for
/// TOML, in-process, and rustfmt for Rust.
#[tokio::test]
async fn built_in_formatting_formats_edits() {
    let dir = std::env::temp_dir().join(format!("sem-edit-built-in-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("demo.toml"), "[package]\nname = \"demo\"\n").unwrap();
    std::fs::write(dir.join("demo.rs"), "fn main() {\n    let a = 1;\n}\n").unwrap();

    let state = SemanticEditTools::new(dir.clone())
        .unwrap()
        .with_formatting(Formatting::built_in());
    for (file_path, anchor, content) in [
        (
            "demo.toml",
            "name = \"demo\"",
            "name=\"demo\"\nversion=\"0.1.0\"",
        ),
        ("demo.rs", "let a = 1;", "let a=2;"),
    ] {
        let response = tool(serde_json::json!({
            "name": "edit",
            "arguments": {
                "file_path": file_path,
                "operation": "replace",
                "anchor": anchor,
                "content": content,
            }
        }))
        .call_to_text(state.clone(), RequestContext::default())
        .await
        .unwrap();
        let formatter = if file_path.ends_with(".toml") {
            "taplo"
        } else {
            "rustfmt"
        };
        assert!(
            response.contains(&format!("Note: {formatter} formatted the edit.")),
            "{response}"
        );
    }
    let toml = std::fs::read_to_string(dir.join("demo.toml")).unwrap();
    let rust = std::fs::read_to_string(dir.join("demo.rs")).unwrap();

    std::fs::remove_dir_all(&dir).ok();
    assert_eq!(toml, "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n");
    assert_eq!(rust, "fn main() {\n    let a = 2;\n}\n");
}

/// A scratch directory holding `demo.rs`, removed when dropped.
struct Scratch {
    dir: PathBuf,
}

impl Scratch {
    fn new(name: &str, source: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("sem-edit-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("demo.rs"), source).unwrap();
        Self { dir }
    }

    /// A formatter running the shell script `body`, which reads the source on
    /// stdin. The script is run by `sh` rather than executed itself, so it
    /// can't be busy being written when another test's process starts.
    fn script(&self, body: &str) -> Formatter {
        let script = self.dir.join("format.sh");
        std::fs::write(&script, body).unwrap();
        Formatter::command("sh", [script.to_str().unwrap()])
    }

    fn tools(&self, formatter: Formatter) -> SemanticEditTools {
        SemanticEditTools::new(self.dir.clone())
            .unwrap()
            .with_formatting(Formatting::off().with(LanguageName::Rust, formatter))
    }

    fn source(&self) -> String {
        std::fs::read_to_string(self.dir.join("demo.rs")).unwrap()
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.dir).ok();
    }
}

/// `edit` on `demo.rs`: its response, or its error's message.
async fn edit(
    state: &SemanticEditTools,
    operation: &str,
    anchor: &str,
    content: &str,
) -> Result<String, String> {
    tool(serde_json::json!({
        "name": "edit",
        "arguments": {
            "file_path": "demo.rs",
            "operation": operation,
            "anchor": anchor,
            "content": content,
        }
    }))
    .call_to_text(state.clone(), RequestContext::default())
    .await
    .map_err(|error| error.to_string())
}

const REJECTED: &str = "The formatter has encountered the following error making that change, \
so the file has not been modified.";

/// A formatter that can't run is absent: the edit is written as laid out, and
/// the response says why it wasn't formatted.
#[tokio::test]
async fn a_formatter_that_cant_run_leaves_the_edit_unformatted() {
    let scratch = Scratch::new("cant-run", "fn main() {\n    let a = 1;\n}\n");
    let state = scratch.tools(Formatter::command(
        "semantic-edit-test-no-such-formatter",
        ["{path}"],
    ));

    let response = edit(&state, "replace", "let a = 1;", "let a=2;")
        .await
        .unwrap();
    assert_eq!(scratch.source(), "fn main() {\n    let a=2;\n}\n");
    assert!(
        response.contains(
            "Note: semantic-edit-test-no-such-formatter couldn't run (not found on PATH), so this \
edit wasn't formatted; the diff shows exactly what was written."
        ),
        "{response}"
    );
}

/// A formatter that fails on the file as it was before the edit says nothing
/// about the edit: the edit is written unformatted, with a note quoting it.
#[cfg(unix)]
#[tokio::test]
async fn a_formatter_that_fails_without_the_edit_leaves_it_unformatted() {
    let scratch = Scratch::new("always-fails", "fn main() {\n    let a = 1;\n}\n");
    let state = scratch.tools(
        scratch.script("cat >/dev/null\necho 'this formatter refuses every file' >&2\nexit 1\n"),
    );

    let response = edit(&state, "replace", "let a = 1;", "let a=2;")
        .await
        .unwrap();
    assert_eq!(scratch.source(), "fn main() {\n    let a=2;\n}\n");
    assert!(
        response.contains(
            "Note: sh fails on this file even without the edit (this formatter refuses every \
file), so this edit wasn't formatted; the diff shows exactly what was written."
        ),
        "{response}"
    );
}

/// A formatter that formats the file before the edit and fails after it is
/// failing on the edit, which is rejected. An edit it formats is written
/// formatted, and the response names it.
#[cfg(unix)]
#[tokio::test]
async fn a_formatter_that_fails_only_on_the_edit_rejects_it() {
    let original = "fn main() {\n    let a = 1;\n}\n";
    let scratch = Scratch::new("fails-on-edit", original);
    let state = scratch.tools(scratch.script(
        "input=$(cat; echo .)\ninput=${input%.}\ncase \"$input\" in\n  *MARKER*) echo 'MARKER \
isn'\"'\"'t allowed' >&2; exit 1 ;;\nesac\nprintf '%s' \"$input\" | sed 's/let b=2;/let b = 2;/'\n",
    ));

    let error = edit(&state, "insert_after", "let a = 1;", "\n    // MARKER")
        .await
        .unwrap_err();
    assert!(error.starts_with(REJECTED), "{error}");
    assert!(error.contains("MARKER isn't allowed"), "{error}");
    assert_eq!(scratch.source(), original);

    let response = edit(&state, "insert_after", "let a = 1;", "\n    let b=2;")
        .await
        .unwrap();
    assert!(
        response.contains("Note: sh formatted the edit."),
        "{response}"
    );
    assert_eq!(
        scratch.source(),
        "fn main() {\n    let a = 1;\n    let b = 2;\n}\n"
    );
}

/// A formatter's output is validated like the edit's result. Output that fails
/// validation is a failure, judged by the same rule: absent when it is invalid
/// for the file before the edit too, and a rejection when only the edit's is.
#[cfg(unix)]
#[tokio::test]
async fn a_formatter_whose_output_is_invalid_is_judged_per_file() {
    let original = "fn main() {\n    let a = 1;\n}\n";
    let scratch = Scratch::new("invalid-output", original);

    let always = scratch.tools(scratch.script("cat >/dev/null\necho 'fn broken('\n"));
    let response = edit(&always, "replace", "let a = 1;", "let a=2;")
        .await
        .unwrap();
    assert_eq!(scratch.source(), "fn main() {\n    let a=2;\n}\n");
    assert!(
        response.contains(
            "Note: sh fails on this file even without the edit (its output is invalid), so this \
edit wasn't formatted; the diff shows exactly what was written."
        ),
        "{response}"
    );

    std::fs::write(scratch.dir.join("demo.rs"), original).unwrap();
    let on_the_edit = scratch.tools(scratch.script(
        "input=$(cat; echo .)\ninput=${input%.}\ncase \"$input\" in\n  *MARKER*) echo \
'fn broken(' ;;\n  *) printf '%s' \"$input\" ;;\nesac\n",
    ));
    let error = edit(
        &on_the_edit,
        "insert_after",
        "let a = 1;",
        "\n    // MARKER",
    )
    .await
    .unwrap_err();
    assert!(error.starts_with(REJECTED), "{error}");
    assert!(error.contains("Its output is invalid:"), "{error}");
    assert_eq!(scratch.source(), original);
}

/// The formatter runs on the candidate validation picked, and its failure
/// rejects the edit rather than moving it to another candidate. The anchor
/// matches in both functions, and this formatter refuses only the edit to the
/// first: moving on would have edited the second, which nothing asked for.
#[cfg(unix)]
#[tokio::test]
async fn the_formatter_vetoes_the_edit_rather_than_moving_it() {
    let original = "fn a() {\n    let x = 1;\n}\n\nfn b() {\n    let x = 1;\n}\n";
    let scratch = Scratch::new("veto", original);
    let state = scratch.tools(scratch.script(
        "input=$(cat; echo .)\ninput=${input%.}\ncase \"$(printf '%s' \"$input\" | grep -m1 \
'let x')\" in\n  *2*) echo 'refusing the change to a' >&2; exit 1 ;;\nesac\nprintf '%s' \
\"$input\"\n",
    ));

    let error = edit(&state, "replace", "let x = 1;", "let x = 2;")
        .await
        .unwrap_err();
    assert!(error.starts_with(REJECTED), "{error}");
    assert!(error.contains("refusing the change to a"), "{error}");
    assert_eq!(scratch.source(), original);
}

/// A built-in formatter runs the way its project would run it: in the file's
/// directory, given the file's path, so it finds the project's configuration,
/// and without flags that would override it. This process's own working
/// directory, the crate's, has none of these files.
#[tokio::test]
async fn built_in_formatters_use_the_files_project_configuration() {
    let dir = std::env::temp_dir().join(format!("sem-edit-project-{}", std::process::id()));
    let src = dir.join("src");
    std::fs::create_dir_all(&src).unwrap();
    std::fs::write(dir.join("rustfmt.toml"), "hard_tabs = true\n").unwrap();
    std::fs::write(
        dir.join("ruff.toml"),
        "extend-exclude = [\"src/vendored.py\"]\n[format]\nquote-style = \"single\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.join("biome.json"),
        "{ \"formatter\": { \"indentStyle\": \"space\", \"indentWidth\": 8 } }\n",
    )
    .unwrap();
    let files = [
        (
            "src/lib.rs",
            "fn f() {\n    let a = 1;\n}\n",
            "let a = 1;",
            "let a = 2;",
        ),
        ("src/module.py", "x = \"a\"\n", "x = \"a\"", "x = \"b\""),
        ("src/vendored.py", "x = \"a\"\n", "x = \"a\"", "x = \"b\""),
        (
            "src/app.ts",
            "function f() {\n  return 1;\n}\n",
            "return 1;",
            "return 2;",
        ),
    ];
    for (file, source, _, _) in files {
        std::fs::write(dir.join(file), source).unwrap();
    }

    let state = SemanticEditTools::new(dir.clone())
        .unwrap()
        .with_formatting(Formatting::built_in());
    for (file, _, anchor, content) in files {
        tool(serde_json::json!({
            "name": "edit",
            "arguments": {
                "file_path": file,
                "operation": "replace",
                "anchor": anchor,
                "content": content,
            }
        }))
        .call_to_text(state.clone(), RequestContext::default())
        .await
        .unwrap();
    }
    let read = |file: &str| std::fs::read_to_string(dir.join(file)).unwrap();
    let edited = files.map(|(file, ..)| read(file));
    std::fs::remove_dir_all(&dir).ok();

    assert_eq!(
        edited,
        [
            // rustfmt.toml's tabs.
            "fn f() {\n\tlet a = 2;\n}\n",
            // ruff.toml's quotes, and its exclusion left as it is.
            "x = 'b'\n",
            "x = \"b\"\n",
            // biome.json's indentation, not the file's own.
            "function f() {\n        return 2;\n}\n",
        ]
    );
}

/// A Rust file that rustfmt wouldn't leave as it is far from its `let a = 1;`.
const CHURN: &str = "fn misindented() {\n        let x = 1;\n}\n\n// a\n// b\n// c\n// d\n// e\n// f\n\nfn main() {\n    let a = 1;\n}\n";

/// By default a formatter's changes are kept to the edit: the edit is
/// formatted, and the rest of the file is left as it was, even where the
/// formatter would change it.
#[tokio::test]
async fn by_default_the_formatter_formats_the_edit_alone() {
    let scratch = Scratch::new("edit-alone", CHURN);
    let state = SemanticEditTools::new(scratch.dir.clone())
        .unwrap()
        .with_formatting(Formatting::off().with(LanguageName::Rust, Formatter::rustfmt()));

    let response = edit(&state, "replace", "let a = 1;", "let a=2;")
        .await
        .unwrap();
    assert_eq!(scratch.source(), CHURN.replace("let a = 1;", "let a = 2;"));
    assert!(
        response.contains("\nNote: rustfmt formatted the edit.\n"),
        "{response}"
    );
    // Nothing outside the edit changed, so there is nothing to leave out.
    assert!(!response.contains("formatting-only"), "{response}");
    assert!(
        response.ends_with(
            "===DIFF===\n@@ -10,5 +10,5 @@\n // f\n\n fn main() {\n-    let a = 1;\n+    let a = 2;\n }"
        ),
        "{response}"
    );
}

/// With whole-file formatting, every change the formatter makes is kept, as
/// it always was, and the whitespace-only ones far from the edit are counted
/// rather than shown.
#[tokio::test]
async fn with_whole_file_the_formatter_formats_the_whole_file() {
    let scratch = Scratch::new("whole-file", CHURN);
    let state = SemanticEditTools::new(scratch.dir.clone())
        .unwrap()
        .with_formatting(
            Formatting::off()
                .with(LanguageName::Rust, Formatter::rustfmt())
                .with_whole_file(true),
        );

    let response = edit(&state, "replace", "let a = 1;", "let a=2;")
        .await
        .unwrap();
    assert_eq!(
        scratch.source(),
        CHURN
            .replace("        let x", "    let x")
            .replace("let a = 1;", "let a = 2;")
    );
    assert!(
        response.contains(
            "\nNote: rustfmt formatted the whole file, including the edit.\n1 formatting-only \
hunk elsewhere in the file is not shown"
        ),
        "{response}"
    );
    assert!(!response.contains("let x"), "{response}");
}

/// Whole-file formatting can be turned on while the tools run, like the rest
/// of the settings.
#[tokio::test]
async fn whole_file_can_change_while_the_tools_run() {
    let scratch = Scratch::new("whole-file-change", CHURN);
    let formatting = Formatting::off().with(LanguageName::Rust, Formatter::rustfmt());
    let state = SemanticEditTools::new(scratch.dir.clone())
        .unwrap()
        .with_formatting(formatting.clone());

    edit(&state, "replace", "let a = 1;", "let a=2;")
        .await
        .unwrap();
    assert!(scratch.source().contains("        let x = 1;"));

    state.set_formatting(formatting.with_whole_file(true));
    edit(&state, "replace", "let a = 2;", "let a=3;")
        .await
        .unwrap();
    assert_eq!(
        scratch.source(),
        CHURN
            .replace("        let x", "    let x")
            .replace("let a = 1;", "let a = 3;")
    );
}

/// A formatter's changes to the edit may only make sense with its changes
/// elsewhere. This one moves the brace that closes `a` from its own line to
/// the end of the file, which takes a change far from the edit and one beside
/// it: the one beside it alone leaves a stray brace. Then the edit is written
/// as laid out, with a note, rather than rejected, or formatted along with the
/// rest of the file.
#[cfg(unix)]
#[tokio::test]
async fn changes_the_edit_cant_have_without_the_rest_leave_it_unformatted() {
    let original = "fn a() {\n    let x = 1;\n}\n// 1\n// 2\n// 3\n// 4\n// 5\nfn main() {\n    let b = 1;\n}\n";
    let scratch = Scratch::new("coupled", original);
    let formatter = scratch.script("sed -e '3d' -e 's/let b=2;/let b = 2;/' -e '$a\\\n}'\n");

    let response = edit(
        &scratch.tools(formatter.clone()),
        "replace",
        "let b = 1;",
        "let b=2;",
    )
    .await
    .unwrap();
    assert_eq!(scratch.source(), original.replace("let b = 1;", "let b=2;"));
    assert!(
        response.contains(
            "\nNote: sh's changes couldn't be applied to the edit alone, so this edit wasn't \
formatted; the diff shows exactly what was written.\n"
        ),
        "{response}"
    );

    // With whole-file formatting, all of them are kept, and the result is
    // valid.
    std::fs::write(scratch.dir.join("demo.rs"), original).unwrap();
    let state = SemanticEditTools::new(scratch.dir.clone())
        .unwrap()
        .with_formatting(
            Formatting::off()
                .with(LanguageName::Rust, formatter)
                .with_whole_file(true),
        );
    let response = edit(&state, "replace", "let b = 1;", "let b=2;")
        .await
        .unwrap();
    assert!(
        response.contains("\nNote: sh formatted the whole file, including the edit.\n"),
        "{response}"
    );
    assert_eq!(
        scratch.source(),
        "fn a() {\n    let x = 1;\n// 1\n// 2\n// 3\n// 4\n// 5\nfn main() {\n    let b = 2;\n}\n}\n"
    );
}
