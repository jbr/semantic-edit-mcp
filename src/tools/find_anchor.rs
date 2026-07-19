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

/// Show every location an anchor matches in a file, with surrounding context
///
/// Uses the same whitespace-insensitive matching as `preview_edit`, without
/// making any changes — useful for checking that an anchor matches where you
/// mean it to, and that it matches only once, before editing.
#[derive(Serialize, Deserialize, Debug, JsonSchema, clap::Args)]
#[serde(rename = "find_anchor")]
#[group(skip)]
pub struct FindAnchor {
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

impl WithExamples for FindAnchor {
    fn examples() -> Vec<Example<Self>> {
        vec![
            Example {
                description: "Check where a function anchor matches",
                item: Self {
                    file_path: "src/main.rs".into(),
                    anchor: "fn main() {".into(),
                    language: None,
                },
            },
            Example {
                description: "Check that a struct anchor is unique",
                item: Self {
                    file_path: "src/lib.rs".into(),
                    anchor: "struct User".into(),
                    language: None,
                },
            },
            Example {
                description: "Locate a comment before anchoring an edit to it",
                item: Self {
                    file_path: "src/utils.rs".into(),
                    anchor: "// Helper function".into(),
                    language: None,
                },
            },
        ]
    }
}

impl Tool<SemanticEditTools> for FindAnchor {
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

        // The operation and content are irrelevant here: find_anchor reports
        // where the anchor text matches, nothing more.
        let selector = crate::selector::Selector {
            operation: crate::selector::Operation::Replace,
            anchor,
        };

        let editor = Editor::new(String::new(), selector, language, file_path, None)?;

        editor.find_anchor()
    }
}
