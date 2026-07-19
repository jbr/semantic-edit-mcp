use crate::editor::Editor;
use crate::education;
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

        // Observations for the education layer, taken before this call
        // replaces the staged operation.
        let education = state.education(None)?;
        let resent_content = matches!(
            (state.get_staged_operation(None)?, content.as_deref()),
            (Some(prev), Some(new)) if *prev.file_path() == file_path
                && prev.selector() != &selector
                && prev.content() == new
        );

        let content = content.unwrap_or_default();
        let language = state
            .language_registry()
            .get_language_with_hint(&file_path, language)?;

        let editor = Editor::new(
            content.clone(),
            selector.clone(),
            language,
            file_path.clone(),
            None,
        )?;
        let shorthand = editor.shorthand_suggestion();
        let (mut message, staged_operation) = editor.preview()?;
        let success = staged_operation.is_some();

        // Never teach an unverified shorthand: re-run the edit with the
        // shortened anchor and only offer it if the diff is identical.
        let verified_shorthand = if success && education.prefix_tip_due() {
            shorthand.filter(|short| {
                Editor::new(
                    content.clone(),
                    Selector {
                        operation: selector.operation,
                        anchor: short.clone(),
                    },
                    language,
                    file_path.clone(),
                    None,
                )
                .and_then(Editor::preview)
                .map(|(short_message, _)| Editor::equivalent_diffs(&message, &short_message))
                .unwrap_or(false)
            })
        } else {
            None
        };

        state.preview_edit(None, staged_operation)?;

        if success {
            let tip = if resent_content && education.retarget_tip_due(content.len()) {
                state.update_education(None, |education| {
                    education.record_success();
                    education.record_retarget_tip_emitted();
                })?;
                Some(education::retarget_tip())
            } else if let Some(short) = verified_shorthand {
                state.update_education(None, |education| {
                    education.record_success();
                    education.record_prefix_tip_emitted();
                })?;
                Some(education::prefix_tip(&short))
            } else {
                state.update_education(None, |education| education.record_success())?;
                None
            };
            if let Some(tip) = tip {
                message.push_str("\n\n");
                message.push_str(&tip);
            }
        }

        Ok(message)
    }
}
