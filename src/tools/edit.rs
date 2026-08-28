use crate::editor::Editor;
use crate::education;
use crate::languages::LanguageName;
use crate::selector::{Operation, Selector};
use crate::state::{AppliedEdit, SemanticEditTools};
use anyhow::{Result, bail};
use mcplease::{
    traits::{Tool, ToolMeta},
    types::{Example, RequestContext, ToolAnnotations},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Apply an edit to a file and see the resulting diff
///
/// Find the `anchor` text in the file and apply `operation` at that location.
/// Every edit is validated before it is written: the result must still parse
/// in the file's language and pass its formatter, or the edit is rejected and
/// the file is untouched. A valid edit is written to disk immediately —
/// review the returned diff, and if the edit landed somewhere other than
/// intended, `retarget_edit` moves it to a corrected anchor and `undo_edit`
/// reverts it.
///
/// The whole file is reformatted with the language's standard formatter, so
/// the diff may include formatting fixes beyond the edit itself.
#[derive(Serialize, Deserialize, Debug, JsonSchema, clap::Args)]
#[serde(rename = "edit")]
#[group(skip)]
pub struct Edit {
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

impl ToolMeta for Edit {
    fn title() -> Option<&'static str> {
        Some("Edit code")
    }

    fn annotations() -> Option<ToolAnnotations> {
        Some(ToolAnnotations {
            read_only_hint: Some(false),
            // Replaces or deletes existing code, though `undo_edit` reverts the
            // most recent one.
            destructive_hint: Some(true),
            // Re-sending the same edit is guarded against, but an insert
            // repeated after another edit intervenes does duplicate content.
            idempotent_hint: Some(false),
            // Only touches files in the session's working directory.
            open_world_hint: Some(false),
            ..Default::default()
        })
    }

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

/// How this request relates to the last applied edit, when that record still
/// describes the file's current contents. Because edits persist immediately,
/// re-sending an already-applied edit is no longer harmless the way
/// re-previewing was: it would double the change on disk.
enum DuplicateOfLastEdit {
    /// Identical selector and content: applying again would apply the change
    /// twice, and the file already contains it.
    ExactResend,
    /// Identical non-empty content re-sent as a `replace` at a different
    /// anchor: almost certainly an attempt to *move* the previous edit, and
    /// applying it would both duplicate the content and overwrite the undo
    /// record — losing the only copy of whatever the first replace clobbered.
    MovedReplace,
    /// Identical non-empty content inserted at a different location: possibly
    /// intentional duplication, so it proceeds, but with a warning that the
    /// earlier placement is still in the file.
    DuplicatedInsert,
}

fn duplicate_of_last_edit(
    last_edit: Option<&AppliedEdit>,
    file_path: &std::path::Path,
    selector: &Selector,
    content: Option<&str>,
    owns_persistence: bool,
) -> Option<DuplicateOfLastEdit> {
    let last = last_edit?;
    let op = last.operation();
    if op.file_path() != file_path {
        return None;
    }
    // Only compare against a record that still describes the file's current
    // contents; if the file has moved on (or the record is stale), a repeated
    // edit is a fresh edit. When an embedder owns persistence the disk isn't
    // observable, so the record is trusted as current.
    if owns_persistence && !last.is_current_on_disk().unwrap_or(false) {
        return None;
    }

    let content = content.unwrap_or_default();
    if op.selector() == selector && op.content() == content {
        return Some(DuplicateOfLastEdit::ExactResend);
    }
    if content.is_empty() || op.content() != content {
        return None;
    }
    if op.selector().operation == Operation::Replace && selector.operation == Operation::Replace {
        Some(DuplicateOfLastEdit::MovedReplace)
    } else {
        Some(DuplicateOfLastEdit::DuplicatedInsert)
    }
}

impl Tool<SemanticEditTools> for Edit {
    type Output = String;

    fn execute(self, state: &mut SemanticEditTools, _context: &RequestContext) -> Result<String> {
        let Self {
            file_path,
            selector,
            content,
            language,
        } = self;

        let file_path = state.resolve_path(&file_path, None)?;
        let education = state.education(None)?;

        let owns_persistence = state.owns_persistence();
        let duplicate = duplicate_of_last_edit(
            state.last_edit(None)?,
            &file_path,
            &selector,
            content.as_deref(),
            owns_persistence,
        );
        match duplicate {
            // A success: the file already holds exactly what this call asked
            // for, so the requested state is true and there is nothing to act
            // on. The `MovedReplace` case below is the opposite — the content
            // is not where this call asked for it — so that one errors.
            Some(DuplicateOfLastEdit::ExactResend) => {
                return Ok("This exact edit was already applied — the file already contains \
this change, so it was not applied again. If the previous application was unintended, \
`undo_edit` reverts it."
                    .to_string());
            }
            Some(DuplicateOfLastEdit::MovedReplace) => {
                bail!(
                    "The file was not modified: this content is identical to the \
`replace` just applied to this file at a different anchor. If that edit landed on the wrong \
target, use `retarget_edit` with the corrected anchor — it reverts the earlier placement and \
re-applies the content there in one step. (`undo_edit` also reverts it.)"
                );
            }
            Some(DuplicateOfLastEdit::DuplicatedInsert) | None => {}
        }

        let content = content.unwrap_or_default();
        let language = state
            .language_registry()
            .get_language_with_hint(&file_path, language)?;

        let editor = Editor::new(
            content.clone(),
            selector.clone(),
            language,
            file_path.clone(),
        )?;
        let shorthand = editor.shorthand_suggestion();
        let (mut message, applied) = editor.apply()?;

        // A rejected edit is an error, not a success whose body describes a
        // failure: the file is untouched, and the caller has to change what it
        // sent. The rejection report is the error's message, unchanged.
        let Some(applied) = applied else {
            bail!("{message}");
        };

        // Never teach an unverified shorthand: re-run the edit with the
        // shortened anchor and only offer it if the diff is identical. This
        // runs before the edit is persisted, so both runs see the same file.
        let verified_shorthand = if education.prefix_tip_due() {
            shorthand.filter(|short| {
                Editor::new(
                    content.clone(),
                    Selector {
                        operation: selector.operation,
                        anchor: short.clone(),
                    },
                    language,
                    file_path.clone(),
                )
                .and_then(Editor::apply)
                .map(|(short_message, _)| Editor::equivalent_diffs(&message, &short_message))
                .unwrap_or(false)
            })
        } else {
            None
        };

        state.persist_output(file_path, applied.post_edit_source.clone())?;
        message = format!(
            "Applied {} — the file has been updated.\n{message}",
            applied.operation().selector().operation_name()
        );
        state.set_last_edit(None, Some(applied))?;
        state.update_education(None, |education| education.record_success())?;

        if matches!(duplicate, Some(DuplicateOfLastEdit::DuplicatedInsert)) {
            message.push_str("\n\n");
            message.push_str(education::duplicate_insert_warning());
        } else if let Some(short) = verified_shorthand {
            state.update_education(None, |education| education.record_prefix_tip_emitted())?;
            message.push_str("\n\n");
            message.push_str(&education::prefix_tip(&short));
        }

        Ok(message)
    }
}
