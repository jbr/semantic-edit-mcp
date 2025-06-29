use crate::editor::Editor;
use crate::state::SemanticEditTools;
use anyhow::{anyhow, Result};
use mcplease::traits::{Tool, WithExamples};
use mcplease::types::Example;
use serde::{Deserialize, Serialize};

/// Execute the currently staged operation
#[derive(Serialize, Deserialize, Debug, schemars::JsonSchema)]
#[serde(rename = "commit_staged")]
pub struct CommitStaged {
    /// Confirm that you want to execute the staged operation
    #[serde(default = "default_acknowledge")]
    pub acknowledge: bool,
    // this is commented out temporarily as an experiment in usability
    // /// Optional session identifier
    // pub session_id: Option<String>,
}

fn default_acknowledge() -> bool {
    true
}

impl WithExamples for CommitStaged {
    fn examples() -> Vec<Example<Self>> {
        vec![Example {
            description: "Commit the currently staged operation",
            item: Self { acknowledge: true },
        }]
    }
}

impl Tool<SemanticEditTools> for CommitStaged {
    fn execute(self, state: &mut SemanticEditTools) -> Result<String> {
        let Self { acknowledge } = self;

        if !acknowledge {
            return Err(anyhow!("Operation not acknowledged"));
        }

        let staged_operation = state
            .take_staged_operation(None)?
            .ok_or_else(|| anyhow!("No operation is currently staged"))?;

        let editor = Editor::from_staged_operation(staged_operation, &state.language_registry())?;
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
