use std::borrow::Cow;

use crate::editor::clean_diff;
use crate::selector::Selector;
use crate::state::SemanticEditTools;
use anyhow::{Result, anyhow};
use mcplease::traits::{Tool, ToolMeta};
use mcplease::types::{Example, RequestContext, ToolAnnotations};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Revert the most recent edit to a file
///
/// Restores the file to its exact content from before its most recent `edit`
/// (or `retarget_edit`). Calling it again reverts the edit before that, as
/// long as the file has not changed in between; the most recent edits across
/// all files are held. Fails without changing anything if the file has been
/// modified by something else since the edit was applied.
#[derive(Serialize, Deserialize, Debug, clap::Args)]
#[serde(rename = "undo_edit")]
#[group(skip)]
pub struct UndoEdit {
    /// The file to revert: absolute, or relative to the server's root directory.
    pub file_path: String,
}

impl JsonSchema for UndoEdit {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("undo_edit")
    }

    fn json_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "description": "Revert the most recent edit to a file, restoring its prior content",
            "type": "object",
            "properties": {
                "file_path": {
                    "type": "string",
                    "description": "The file to revert: absolute, or relative to the server's root directory."
                }
            },
            "required": ["file_path"]
        })
    }
}

impl ToolMeta for UndoEdit {
    fn title() -> Option<&'static str> {
        Some("Undo edit")
    }

    fn annotations() -> Option<ToolAnnotations> {
        Some(ToolAnnotations {
            read_only_hint: Some(false),
            // Discards the edit's changes.
            destructive_hint: Some(true),
            // A second call reverts the edit before, not the same one again.
            idempotent_hint: Some(false),
            open_world_hint: Some(false),
            ..Default::default()
        })
    }

    fn examples() -> Vec<Example<Self>> {
        vec![Example {
            description: "Revert the edit that was just applied to a file",
            item: Self {
                file_path: "src/lib.rs".into(),
            },
        }]
    }
}

impl Tool<SemanticEditTools> for UndoEdit {
    type Output = String;

    async fn execute(self, state: SemanticEditTools, _context: RequestContext) -> Result<String> {
        state.unblock(move |state| self.run(state)).await
    }
}

impl UndoEdit {
    pub(crate) fn run(self, state: &SemanticEditTools) -> Result<String> {
        let file_path = state.resolve_path(&self.file_path)?;
        let _write_lock = state.write_lock();
        let display_path = state.display_path(&file_path).to_path_buf();
        let record = state
            .latest_edit(&file_path)
            .ok_or_else(|| anyhow!("No edit to {} is held for undo.", display_path.display()))?;

        if state.owns_persistence() {
            match record.is_current_on_disk() {
                Ok(true) => {}
                Ok(false) => {
                    return Err(anyhow!(
                        "Cannot undo: {} has changed since its most recent edit was applied, \
and undoing would discard those later changes. The file was not modified.",
                        display_path.display()
                    ));
                }
                Err(error) => {
                    return Err(anyhow!(
                        "Cannot undo: failed to read {}: {error}",
                        display_path.display()
                    ));
                }
            }
        }

        state.persist_output(file_path.clone(), record.pre_edit_source.clone())?;
        state.forget_latest_edit(&file_path);

        Ok(format!(
            "Reverted the most recent edit to {} ({} at {}) — it has been restored to its prior \
content.\n\n{}",
            display_path.display(),
            record.operation().selector().operation_name(),
            target_summary(record.operation().selector()),
            clean_diff(record.post_edit_source(), record.pre_edit_source())
        ))
    }
}

/// How the reverted edit's target is named back: the anchor's first line —
/// enough to identify which edit it was without replaying a long anchor into the
/// conversation — or, for an item-targeted edit, the item reference itself.
fn target_summary(selector: &Selector) -> String {
    match (selector.anchor(), &selector.item) {
        (Some(anchor), _) => format!("{:?}", first_line(anchor)),
        (None, Some(item)) => item.to_string(),
        (None, None) => String::from("its target"),
    }
}

fn first_line(anchor: &str) -> &str {
    anchor.trim().lines().next().unwrap_or_default()
}
