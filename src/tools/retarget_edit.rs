use crate::{editor::Editor, selector::Selector, state::SemanticEditTools};

use anyhow::{Result, anyhow};
use mcplease::{
    traits::{Tool, WithExamples},
    types::Example,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Move the most recent edit to a different location without resending its content
///
/// Reverts the last edit and re-applies its content at the corrected anchor
/// in a single step. Useful when the diff from `edit` shows the change
/// landing somewhere other than intended. If the edit fails at the new
/// anchor, the file is left unchanged, with the previous placement still
/// applied and still retargetable.
#[derive(Serialize, Deserialize, Debug, JsonSchema, clap::Args)]
#[serde(rename = "retarget_edit")]
#[group(skip)]
pub struct RetargetEdit {
    #[serde(flatten)]
    #[clap(flatten)]
    pub selector: Selector,
}

impl WithExamples for RetargetEdit {
    fn examples() -> Vec<Example<Self>> {
        vec![]
    }
}

impl Tool<SemanticEditTools> for RetargetEdit {
    fn execute(self, state: &mut SemanticEditTools) -> Result<String> {
        let Self { selector } = self;

        let record = state
            .last_edit(None)?
            .cloned()
            .ok_or_else(|| anyhow!("No edit to retarget. Only the most recent edit is retargetable."))?;

        let file_path = record.operation.file_path.clone();
        if state.owns_persistence() {
            match record.is_current_on_disk() {
                Ok(true) => {}
                Ok(false) => {
                    return Err(anyhow!(
                        "Cannot retarget: {} has changed since the last edit was applied, \
so that edit can no longer be safely moved. The file was not modified.",
                        file_path.display()
                    ));
                }
                Err(error) => {
                    return Err(anyhow!(
                        "Cannot retarget: failed to read {}: {error}",
                        file_path.display()
                    ));
                }
            }
        }

        if *record.operation().selector() == selector {
            return Ok(
                "The last edit already used exactly this targeting; nothing to move.".to_string(),
            );
        }

        // Re-run the edit against the recorded *pre-edit* source: the file on
        // disk still contains the placement being moved, and it only gets
        // rewritten once the new placement validates.
        let language = state
            .language_registry()
            .get_language(record.operation().language_name);
        let editor = Editor::from_source(
            record.operation.content.clone(),
            selector,
            language,
            file_path.clone(),
            record.pre_edit_source.clone(),
        )?;

        let (message, applied) = editor.apply()?;
        match applied {
            Some(applied) => {
                state.persist_output(file_path, applied.post_edit_source.clone())?;
                let message = format!(
                    "Retargeted {}: the previous placement was reverted and the edit was \
re-applied at the new anchor. The diff is relative to the file from before the original edit.\n{message}",
                    applied.operation().selector().operation_name()
                );
                state.set_last_edit(None, Some(applied))?;
                Ok(message)
            }
            None => Ok(format!(
                "Retarget failed — the file is unchanged and the previous placement remains \
applied.\n\n{message}"
            )),
        }
    }
}
