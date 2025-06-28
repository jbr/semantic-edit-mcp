use crate::editor::Editor;
use crate::languages::LanguageName;
use crate::selector::{Operation, Selector};
use crate::state::SemanticEditTools;
use crate::traits::WithExamples;
use crate::types::Example;
use anyhow::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Stage an operation for execution and preview what it would do
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
    fn examples() -> Option<Vec<Example<Self>>> {
        Some(vec![
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
        ])
    }
}

impl StageOperation {
    pub(crate) fn execute(self, state: &mut SemanticEditTools) -> Result<String> {
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
