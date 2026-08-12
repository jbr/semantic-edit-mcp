use std::borrow::Cow;
use std::path::Path;

use crate::editor::clean_diff;
use crate::state::SemanticEditTools;
use anyhow::{Result, anyhow};
use mcplease::traits::{Tool, WithExamples};
use mcplease::types::Example;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Revert the most recent edit
///
/// Restores the edited file to its exact content from before the last `edit`
/// (or `retarget_edit`). Only the single most recent edit is held for undo —
/// each new edit replaces the undo state. Fails without changing anything if
/// the file has been modified by something else since the edit was applied.
#[derive(Serialize, Deserialize, Debug, clap::Args)]
#[serde(rename = "undo_edit")]
#[group(skip)]
pub struct UndoEdit {}

impl JsonSchema for UndoEdit {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("undo_edit")
    }

    fn json_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "description": "Revert the most recent edit, restoring the file's prior content",
            "type": "object",
            "properties": {}
        })
    }
}

impl WithExamples for UndoEdit {
    fn examples() -> Vec<Example<Self>> {
        vec![Example {
            description: "Revert the edit that was just applied",
            item: Self {},
        }]
    }
}

impl Tool<SemanticEditTools> for UndoEdit {
    fn execute(self, state: &mut SemanticEditTools) -> Result<String> {
        let record = state
            .last_edit(None)?
            .cloned()
            .ok_or_else(|| anyhow!("No edit to undo. Only the most recent edit is undoable."))?;

        let file_path = record.operation.file_path.clone();
        if state.owns_persistence() {
            match record.is_current_on_disk() {
                Ok(true) => {}
                Ok(false) => {
                    return Err(anyhow!(
                        "Cannot undo: {} has changed since the last edit was applied, and \
undoing would discard those later changes. The file was not modified.",
                        file_path.display()
                    ));
                }
                Err(error) => {
                    return Err(anyhow!(
                        "Cannot undo: failed to read {}: {error}",
                        file_path.display()
                    ));
                }
            }
        }

        // Display the path relative to the session root when possible: the
        // absolute path is machine-specific noise in the response (and in
        // snapshot expectations).
        let display_path = state
            .get_context(None)
            .ok()
            .flatten()
            .and_then(|context| std::fs::canonicalize(context).ok())
            .and_then(|context| file_path.strip_prefix(context).ok().map(Path::to_path_buf))
            .unwrap_or_else(|| file_path.clone());

        state.persist_output(file_path, record.pre_edit_source.clone())?;
        state.set_last_edit(None, None)?;

        Ok(format!(
            "Reverted the last edit ({} at {:?}) — {} has been restored to its prior content.\n\n{}",
            record.operation().selector().operation_name(),
            first_line(&record.operation().selector().anchor),
            display_path.display(),
            clean_diff(record.post_edit_source(), record.pre_edit_source())
        ))
    }
}

/// The anchor's first line, enough to identify which edit was reverted
/// without replaying a long anchor back into the conversation.
fn first_line(anchor: &str) -> &str {
    anchor.trim().lines().next().unwrap_or_default()
}
