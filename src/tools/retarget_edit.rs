use crate::{editor::Editor, selector::Selector, state::SemanticEditTools};

use anyhow::{anyhow, Result};
use mcplease::{
    traits::{Tool, WithExamples},
    types::Example,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Re-aim the staged edit at a different location without resending its content
///
/// Useful when a `preview_edit` diff shows the edit landing in the wrong
/// place: restate the operation with a corrected anchor and the staged content
/// is re-applied there. A failed retarget leaves the previously staged edit in
/// place.
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

        let staged_operation = state
            .modify_staged_operation(None, |op| op.retarget(selector))?
            .ok_or_else(|| anyhow!("no operation staged"))?;

        let editor =
            Editor::from_staged_operation(staged_operation.clone(), state.language_registry())?;
        let (message, staged_operation) = editor.preview()?;
        if staged_operation.is_some() {
            // leave failed operations in place
            state.preview_edit(None, staged_operation)?;
        }
        Ok(message)
    }
}
