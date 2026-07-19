use crate::editor::Editor;
use crate::languages::LanguageName;
use crate::state::SemanticEditTools;
use anyhow::Result;
use mcplease::{
    traits::{Tool, WithExamples},
    types::Example,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Show the code an anchor targets, with surrounding context
///
/// Uses the same anchor matching as `preview_edit`, without making any
/// changes — useful for checking what an anchor resolves to before editing.
#[derive(Serialize, Deserialize, Debug, JsonSchema, clap::Args)]
#[serde(rename = "read_node")]
#[group(skip)]
pub struct ReadNode {
    /// Path to the source file
    #[serde(rename = "file_path")]
    pub file_path: String,

    /// Text to locate in the source code, matched whitespace-insensitively, as
    /// in `preview_edit`.
    pub anchor: String,

    /// Optional language hint (e.g., "rust", "javascript")
    #[serde(skip_serializing_if = "Option::is_none")]
    pub language: Option<LanguageName>,
}

impl WithExamples for ReadNode {
    fn examples() -> Vec<Example<Self>> {
        vec![
            Example {
                description: "Read a function definition",
                item: Self {
                    file_path: "src/main.rs".into(),
                    anchor: "fn main() {".into(),
                    language: None,
                },
            },
            Example {
                description: "Read a struct definition",
                item: Self {
                    file_path: "src/lib.rs".into(),
                    anchor: "struct User".into(),
                    language: None,
                },
            },
            Example {
                description: "Read a comment section",
                item: Self {
                    file_path: "src/utils.rs".into(),
                    anchor: "// Helper function".into(),
                    language: None,
                },
            },
        ]
    }
}

impl Tool<SemanticEditTools> for ReadNode {
    fn execute(self, state: &mut SemanticEditTools) -> Result<String> {
        let Self {
            file_path,
            anchor,
            language,
        } = self;

        let file_path = state.resolve_path(&file_path, None)?;

        let language = state
            .language_registry()
            .get_language_with_hint(&file_path, language)?;

        let source_code = std::fs::read_to_string(&file_path)?;

        // Create a selector with Operation::Replace to use for node finding
        // We won't actually perform the operation, just use it to locate the node
        let selector = crate::selector::Selector {
            operation: crate::selector::Operation::Replace,
            anchor,
        };

        let editor = Editor::new(
            source_code.clone(),
            selector,
            language,
            file_path,
            None,
        )?;

        let output = editor.read_node()?;

        Ok(output)
    }
}
