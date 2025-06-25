use crate::traits::WithExamples;
use crate::types::Example;
use crate::{operations::ExecutionResult, state::SemanticEditTools};
use anyhow::{anyhow, Result};
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
    fn examples() -> Option<Vec<Example<Self>>> {
        Some(vec![Example {
            description: "Commit the currently staged operation",
            item: Self { acknowledge: true },
        }])
    }
}

impl CommitStaged {
    pub(crate) fn execute(self, state: &mut SemanticEditTools) -> Result<String> {
        let Self { acknowledge } = self;

        if !acknowledge {
            return Err(anyhow!("Operation not acknowledged"));
        }

        let staged_operation = state
            .take_staged_operation(None)?
            .ok_or_else(|| anyhow!("No operation is currently staged"))?;

        let language = state
            .language_registry()
            .get_language(staged_operation.language_name)
            .ok_or_else(|| anyhow!("language not recognized"))?;

        // Apply the operation for real (preview_only=false)
        let result =
            staged_operation
                .operation()
                .apply(language, staged_operation.file_path(), false)?;

        match result {
            ExecutionResult::Change {
                response,
                output,
                output_path,
            } => {
                if let Some(commit) = state.commit_fn_mut().take() {
                    commit(output_path, output);
                } else {
                    std::fs::write(output_path, output)?;
                }
                Ok(response)
            }
            ExecutionResult::ResponseOnly(response) => Ok(response),
            ExecutionResult::ChangeStaged(_, _) => {
                Err(anyhow!("Unexpected staged result from commit"))
            }
        }
    }
}
