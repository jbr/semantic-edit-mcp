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
/// The Selector uses a simple but powerful approach: find text with `anchor` (and optionally `end`),
/// then perform the specified `operation`. All operations are AST-aware and respect language syntax.
///
/// # Basic Usage
///
/// Most operations only need an `anchor` - a short, unique piece of text to locate:
/// {"operation": "insert_after_node", "anchor": "fn main() {" }
///
/// Replace_range operations also use `end` to specify the extent:
/// { "operation": "replace_range", "anchor": "// Start here", "end": "// End here" }
///
/// To delete a syntax node, use one of the `replace` operations and omit `content`
#[derive(Serialize, Deserialize, Debug, JsonSchema)]
#[serde(rename = "stage_operation")]
pub struct StageOperation {
    /// Path to the source file.
    /// If a session has been configured, this can be a relative path to the session root.
    pub file_path: String,

    /// Optional language hint. If not provided, language will be detected from file extension.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<LanguageName>,

    /// How to position the `content`
    #[serde(flatten)]
    pub selector: Selector,

    /// The new content to insert or replace
    /// IMPORTANT TIP: To remove code, use `replace` and omit `content`
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

impl WithExamples for StageOperation {
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
            //   "operation": "replace_node",
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
                        end: None,
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
                        operation: Operation::ReplaceNode,
                        end: None,
                    },
                    content: Some("fn hello() { println!(\"Hello, world!\"); }".to_string()),
                    language: None,
                },
            },
            Example {
                description: "Replace a range of code with explicit boundaries",
                item: Self {
                    file_path: "src/main.rs".into(),
                    selector: Selector {
                        operation: Operation::ReplaceRange,
                        anchor: "let user =".to_string(),
                        end: Some("return user;".into()),
                    },
                    content: Some(
                        "let user = User::new();\n    validate_user(&user);\n    return user;"
                            .into(),
                    ),
                    language: None,
                },
            },
            Example {
                description: "Removing a function by omitting replacement content",
                item: Self {
                    file_path: "src/main.rs".into(),
                    selector: Selector {
                        operation: Operation::ReplaceNode,
                        anchor: "fn main() {".to_string(),
                        end: None,
                    },
                    content: None,
                    language: None,
                },
            },
        ]
    }
}

impl Tool<SemanticEditTools> for StageOperation {
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
        state.stage_operation(None, staged_operation)?;

        Ok(message)
    }
}
