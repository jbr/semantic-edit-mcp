//! Integration tests for the standalone binary's command line.

use std::{path::PathBuf, process::Command};

/// A Rust file that rustfmt wouldn't leave as it is far from its `let a = 1;`.
const CHURN: &str = "fn misindented() {\n        let x = 1;\n}\n\n// a\n// b\n// c\n// d\n// e\n// f\n\nfn main() {\n    let a = 1;\n}\n";

/// A scratch directory holding `demo.rs`, removed when dropped.
struct Scratch {
    dir: PathBuf,
}

impl Scratch {
    fn new(name: &str) -> Self {
        let dir = std::env::temp_dir().join(format!("sem-edit-bin-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("demo.rs"), CHURN).unwrap();
        Self { dir }
    }

    /// Run the binary's `edit` on `demo.rs` with rustfmt as Rust's formatter,
    /// `before` and `after` the subcommand: its response, and the file after.
    fn edit(&self, before: &[&str], after: &[&str]) -> (String, String) {
        let output = Command::new(env!("CARGO_BIN_EXE_semantic-edit-mcp"))
            .arg("--root")
            .arg(&self.dir)
            .args(["--format", "rust=rustfmt"])
            .args(before)
            .args(["edit", "demo.rs", "replace", "let a = 1;", "let a=2;"])
            .args(after)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let source = std::fs::read_to_string(self.dir.join("demo.rs")).unwrap();
        std::fs::write(self.dir.join("demo.rs"), CHURN).unwrap();
        (String::from_utf8(output.stdout).unwrap(), source)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.dir).ok();
    }
}

/// By default the formatter formats the edit alone; `--format-scope file`,
/// before the subcommand or after it, has it format the whole file.
#[test]
fn format_scope_sets_how_much_of_the_file_the_formatter_changes() {
    let scratch = Scratch::new("format-scope");
    let edited = CHURN.replace("let a = 1;", "let a = 2;");
    let whole_file = edited.replace("        let x", "    let x");

    for before in [&[][..], &["--format-scope", "edit"]] {
        let (response, source) = scratch.edit(before, &[]);
        assert_eq!(source, edited, "{before:?}");
        assert!(
            response.contains("\nNote: rustfmt formatted the edit.\n"),
            "{before:?}: {response}"
        );
    }

    for (before, after) in [
        (&["--format-scope", "file"][..], &[][..]),
        (&[], &["--format-scope=file"]),
        (&["--format-scope", "edit"], &["--format-scope", "file"]),
    ] {
        let (response, source) = scratch.edit(before, after);
        assert_eq!(source, whole_file, "{before:?} {after:?}");
        assert!(
            response.contains("\nNote: rustfmt formatted the whole file, including the edit.\n"),
            "{before:?} {after:?}: {response}"
        );
    }
}
