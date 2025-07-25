use crate::editor::Editor;
use crate::languages::LanguageName;
use crate::selector::{Operation, Selector};
use crate::state::SemanticEditTools;
use anyhow::Result;
use mcplease::{
    traits::{Tool, WithExamples},
    types::Example,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Stage an operation and see a preview of the changes
///
/// The Selector uses a simple but powerful approach: find text with `anchor` (and optionally
/// `end`), then perform the specified `operation`. All operations are AST-aware and respect
/// language syntax. No changes are persisted to disk until you `commit_operation`
#[derive(Serialize, Deserialize, Debug, JsonSchema, clap::Args)]
#[serde(rename = "preview_edit")]
#[group(skip)]
pub struct PreviewEdit {
    /// Path to the source file.
    /// If a session has been configured, this can be a relative path to the session root.
    pub file_path: String,

    /// Optional language hint. If not provided, language will be detected from file extension.
    #[serde(skip_serializing_if = "Option::is_none")]
    #[arg(short, long, value_enum)]
    pub language: Option<LanguageName>,

    /// How to position the `content`
    #[serde(flatten)]
    #[clap(flatten)]
    pub selector: Selector,

    /// The new content to insert or replace
    /// IMPORTANT TIP: To remove code, omit `content`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

impl WithExamples for PreviewEdit {
    fn examples() -> Vec<Example<Self>> {
        vec![
            // more examples to add
            //
            // ```json
            // // Add a new import
            // {
            //   "operation": "insert_after",
            //   "anchor": "use std::collections::HashMap;",
            //   "content": "use std::fs::File;"
            // }
            //
            // // Replace a function body
            // {
            //   "operation": "replace",
            //   "anchor": "fn old_function() {",
            //   "content": "fn new_function() {\n    println!(\"Updated!\");\n}"
            // }
            //
            // // Change a section of code
            // {
            //   "operation": "replace_range",
            //   "anchor": "// TODO: implement this",
            //   "end": "return None;",
            //   "content": "let result = calculate_value();\nreturn Some(result);"
            // }
            //
            Example {
                description: "Insert content after a function declaration",
                item: Self {
                    file_path: "src/main.rs".into(),
                    selector: Selector {
                        anchor: "fn main() {".into(),
                        operation: Operation::InsertAfter,
                    },
                    content: Some("\n    println!(\"Hello, world!\");".to_string()),
                    language: None,
                },
            },
            Example {
                description: "Replace a function with new implementation",
                item: Self {
                    file_path: "src/main.rs".into(),
                    selector: Selector {
                        anchor: "fn hello()".to_string(),
                        operation: Operation::Replace,
                    },
                    content: Some("fn hello() { println!(\"Hello, world!\"); }".to_string()),
                    language: None,
                },
            },
            Example {
                description: "Replace an if statement",
                item: Self {
                    file_path: "src/main.rs".into(),
                    selector: Selector {
                        operation: Operation::Replace,
                        anchor: "if let Some(user) = user {".to_string(),
                    },
                    content: Some("user.map(User::name)".into()),
                    language: None,
                },
            },
            Example {
                description: "Removing a function by omitting replacement content",
                item: Self {
                    file_path: "src/main.rs".into(),
                    selector: Selector {
                        operation: Operation::Replace,
                        anchor: "fn main() {".to_string(),
                    },
                    content: None,
                    language: None,
                },
            },
        ]
    }
}

impl Tool<SemanticEditTools> for PreviewEdit {
    fn execute(self, state: &mut SemanticEditTools) -> Result<String> {
        let Self {
            file_path,
            selector,
            content,
            language,
        } = self;

        let file_path = state.resolve_path(&file_path, None)?;

        let language = state
            .language_registry()
            .get_language_with_hint(&file_path, language)?;

        let editor = Editor::new(
            content.unwrap_or_default(),
            selector,
            language,
            file_path,
            None,
        )?;
        let (message, staged_operation) = editor.preview()?;
        state.preview_edit(None, staged_operation)?;

        Ok(message)
    }
}
