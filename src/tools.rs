use crate::languages::LanguageRegistry;
use crate::operations::{EditOperation, NodeSelector};
use crate::server::{Tool, ToolCallParams};
use crate::staging::{StagedOperation, StagingStore};
use anyhow::{anyhow, Result};
use serde_json::{json, Value};

pub struct ToolRegistry {
    language_registry: LanguageRegistry,
    tools: Vec<Tool>,
}

#[derive(Debug)]
pub enum ExecutionResult {
    ResponseOnly(String),
    ChangeStaged(String, StagedOperation),
    Change {
        response: String,
        output: String,
        output_path: String,
    },
}

impl ExecutionResult {
    pub(crate) async fn write(self) -> Result<String> {
        match self {
            ExecutionResult::ResponseOnly(response) => Ok(response),
            Self::ChangeStaged(response, _) => Ok(response),
            ExecutionResult::Change {
                response,
                output,
                output_path,
            } => {
                tokio::fs::write(output_path, output).await?;
                Ok(response)
            }
        }
    }
}

impl ToolRegistry {
    pub fn new() -> Result<Self> {
        let tools = vec![
            serde_json::from_str(include_str!("../schemas/read_documentation.json"))?,
            serde_json::from_str(include_str!("../schemas/stage_operation.json"))?,
            serde_json::from_str(include_str!("../schemas/retarget_staged.json"))?,
            serde_json::from_str(include_str!("../schemas/commit_staged.json"))?,
        ];

        Ok(Self {
            tools,
            language_registry: LanguageRegistry::new()?,
        })
    }

    pub fn get_tools(&self) -> Vec<Tool> {
        self.tools.clone()
    }

    pub async fn execute_tool(
        &self,
        tool_call: &ToolCallParams,
        staging_store: &StagingStore,
    ) -> Result<ExecutionResult> {
        let empty_args = json!({});
        let args = tool_call.arguments.as_ref().unwrap_or(&empty_args);

        match tool_call.name.as_str() {
            "read_documentation" => self.read_documentation(args),
            "stage_operation" => self.stage_operation(args, staging_store).await,
            "retarget_staged" => self.handle_retarget_staged(args, staging_store).await,
            "commit_staged" => self.commit_staged(staging_store).await,
            tool_call => Err(anyhow!("tool {tool_call} not recognized")),
        }
    }

    fn read_documentation(&self, args: &Value) -> Result<ExecutionResult> {
        let language = args
            .get("language")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("language is required"))?;

        Ok(ExecutionResult::ResponseOnly(
            self.language_registry.get_documentation(language)?,
        ))
    }

    async fn stage_operation(
        &self,
        args: &Value,
        staging_store: &StagingStore,
    ) -> Result<ExecutionResult> {
        let operation_type = args
            .get("operation")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("operation is required"))?;

        let file_path = args
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("file_path is required"))?
            .to_string();

        let language_hint = args
            .get("language")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let target = NodeSelector::new_from_value(args)?;

        let operation = match operation_type {
            "replace" => Ok(EditOperation::Replace { target, content }),
            "remove" => Ok(EditOperation::Delete { target }),
            "insert_before" => Ok(EditOperation::InsertBefore { target, content }),
            "insert_after" => Ok(EditOperation::InsertAfter { target, content }),
            "wrap" => Ok(EditOperation::Wrap {
                target,
                wrapper_template: content,
            }),
            _ => Err(anyhow!("Unknown operation type: {}", operation_type)),
        }?;

        let language = self
            .language_registry
            .get_language_with_hint(&file_path, language_hint.as_deref())?;

        let ExecutionResult::ResponseOnly(message) = operation.apply(language, &file_path, true)?
        else {
            return Err(anyhow!("unexpected change from preview"));
        };
        let staged_operation = StagedOperation {
            operation,
            file_path,
            language_name: language.name(),
        };
        staging_store.stage(staged_operation.clone());
        Ok(ExecutionResult::ChangeStaged(message, staged_operation))
    }

    pub async fn commit_staged(&self, staging_store: &StagingStore) -> Result<ExecutionResult> {
        staging_store
            .take_staged_operation()
            .ok_or_else(|| anyhow!("No operation is currently staged"))?
            .commit(&self.language_registry)
    }

    async fn handle_retarget_staged(
        &self,
        args: &Value,
        staging_store: &StagingStore,
    ) -> std::result::Result<ExecutionResult, anyhow::Error> {
        let selector = NodeSelector::new_from_value(args)?;
        let staged = staging_store
            .modify_staged_operation(|op| op.retarget(selector))
            .ok_or_else(|| anyhow!("no operation staged"))?;
        let language = self
            .language_registry
            .get_language(staged.language_name())
            .ok_or_else(|| anyhow!("language not recognized"))?;

        let ExecutionResult::ResponseOnly(message) =
            staged.operation.apply(language, &staged.file_path, true)?
        else {
            return Err(anyhow!("unexpected change from preview"));
        };

        staging_store.stage(staged.clone());
        Ok(ExecutionResult::ChangeStaged(message, staged))
    }
}
