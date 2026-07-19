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

/// Stage an edit and preview the resulting diff
///
/// Find the `anchor` text in the file and apply `operation` at that location.
/// Nothing is written to disk until you follow up with `persist_edit`.
///
/// Every edit is validated before it is accepted: the result must still parse
/// in the file's language and pass its formatter, or the edit is rejected and
/// the file is untouched. The whole file is reformatted with the language's
/// standard formatter, so the diff may include formatting fixes beyond the
/// edit itself.
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

    /// The new code.
    ///
    /// For `replace`, this takes the place of the anchored code; omit it to
    /// delete the anchored code instead. For inserts, include any blank lines
    /// you want between the new code and its neighbors.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

impl WithExamples for PreviewEdit {
    fn examples() -> Vec<Example<Self>> {
        vec![
            Example {
                description: "Replace a function with a new implementation",
                item: Self {
                    file_path: "src/main.rs".into(),
                    selector: Selector {
                        operation: Operation::Replace,
                        anchor: "fn greet(name: &str) {\n    println!(\"Hello, {name}\");\n}"
                            .into(),
                    },
                    content: Some(
                        "fn greet(name: &str) {\n    println!(\"Hi there, {name}!\");\n}"
                            .to_string(),
                    ),
                    language: None,
                },
            },
            Example {
                description: "Insert a new function after an existing one",
                item: Self {
                    file_path: "src/main.rs".into(),
                    selector: Selector {
                        operation: Operation::InsertAfter,
                        anchor: "fn greet(name: &str) {\n    println!(\"Hello, {name}\");\n}"
                            .into(),
                    },
                    content: Some(
                        "\n\nfn farewell(name: &str) {\n    println!(\"Goodbye, {name}\");\n}"
                            .to_string(),
                    ),
                    language: None,
                },
            },
            Example {
                description: "Replace a single statement",
                item: Self {
                    file_path: "src/main.rs".into(),
                    selector: Selector {
                        operation: Operation::Replace,
                        anchor: "let name = args.next().unwrap();".to_string(),
                    },
                    content: Some(r#"let name = args.next().unwrap_or_default();"#.to_string()),
                    language: None,
                },
            },
            Example {
                description: "Delete a function by omitting content",
                item: Self {
                    file_path: "src/main.rs".into(),
                    selector: Selector {
                        operation: Operation::Replace,
                        anchor: "fn unused_helper() {\n    todo!()\n}".to_string(),
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
