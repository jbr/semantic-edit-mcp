use std::borrow::Cow;

use crate::editor::Editor;
use crate::state::SemanticEditTools;
use anyhow::{anyhow, Result};
use mcplease::traits::{Tool, WithExamples};
use mcplease::types::Example;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Execute the currently staged operation
#[derive(Serialize, Deserialize, Debug, clap::Args)]
#[serde(rename = "persist_edit")]
#[group(skip)]
pub struct PersistEdit {}

impl JsonSchema for PersistEdit {
    fn schema_name() -> Cow<'static, str> {
        Cow::Borrowed("persist_edit")
    }

    fn json_schema(_gen: &mut schemars::SchemaGenerator) -> schemars::Schema {
        schemars::json_schema!({
            "description": "Execute the currently staged operation",
            "type": "object",
            "properties": {}
        })
    }
}

impl WithExamples for PersistEdit {
    fn examples() -> Vec<Example<Self>> {
        vec![Example {
            description: "Commit the currently staged operation",
            item: Self {},
        }]
    }
}

impl Tool<SemanticEditTools> for PersistEdit {
    fn execute(self, state: &mut SemanticEditTools) -> Result<String> {
        let staged_operation = state
            .take_staged_operation(None)?
            .ok_or_else(|| anyhow!("No operation is currently staged"))?;

        let editor = Editor::from_staged_operation(staged_operation, state.language_registry())?;
        let (message, output, output_path) = editor.commit()?;

        if let Some(output) = output {
            if let Some(commit) = state.commit_fn_mut().take() {
                commit(output_path, output);
            } else {
                std::fs::write(output_path, output)?;
            }
        }

        Ok(message)
    }
}
