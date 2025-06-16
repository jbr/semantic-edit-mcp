use crate::languages::LanguageRegistry;
use crate::operations::{EditOperation, NodeSelector};
use crate::server::{Tool, ToolCallParams};
use crate::staging::{StagedOperation, StagingStore};
use anyhow::{Result, anyhow};
use serde_json::{Value, json};

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
            Tool {
                name: "read_documentation",
                description: "🔑 ESSENTIAL: Get all available AST node types for semantic editing. Use this FIRST to discover which ancestor_node_type values are valid for your language (e.g., 'identifier', 'function_item', 'number', 'string'). Required for effective use of stage_operation.",
                input_schema: serde_json::from_str(include_str!(
                    "../schemas/read_documentation.json"
                ))?,
            },
            Tool {
                name: "stage_operation",
                description: "Stage an operation for later execution and preview what it would do. 💡 TIP: Use read_documentation first to see available AST node types for your language!",
                input_schema: serde_json::from_str(include_str!(
                    "../schemas/stage_operation.json"
                ))?,
            },
            Tool {
                name: "commit_staged",
                description: "Execute the currently staged operation",
                input_schema: serde_json::from_str(include_str!("../schemas/commit_staged.json"))?,
            },
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
            "read_documentation" => self.handle_read_documentation(args),
            "stage_operation" => self.handle_stage_operation(args, staging_store).await,
            "commit_staged" => self.handle_commit_staged(staging_store).await,
            tool_call => Err(anyhow!("tool {tool_call} not recognized")),
        }
    }

    fn handle_read_documentation(&self, args: &Value) -> Result<ExecutionResult> {
        let language = args
            .get("language")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("language is required"))?;

        Ok(ExecutionResult::ResponseOnly(
            self.language_registry.get_documentation(language)?,
        ))
    }

    async fn handle_stage_operation(
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

        let selector = NodeSelector::new_from_value(args)?;

        let operation = match operation_type {
            "replace" => self.create_replace_operation(selector, content),
            "remove" => self.create_remove_operation(selector),
            "insert_before" => self.create_insert_before_operation(selector, content),
            "insert_after" => self.create_insert_after_operation(selector, content),
            "wrap" => self.create_wrap_operation(selector, content),
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

    pub async fn handle_commit_staged(
        &self,
        staging_store: &StagingStore,
    ) -> Result<ExecutionResult> {
        staging_store
            .take_staged_operation()
            .ok_or_else(|| anyhow!("No operation is currently staged"))?
            .commit(&self.language_registry)
    }

    fn create_remove_operation(&self, selector: NodeSelector) -> Result<EditOperation> {
        Ok(EditOperation::Delete { target: selector })
    }

    fn create_replace_operation(
        &self,
        selector: NodeSelector,
        content: Option<String>,
    ) -> Result<EditOperation> {
        Ok(EditOperation::Replace {
            target: selector,
            content,
        })
    }

    fn create_insert_before_operation(
        &self,
        selector: NodeSelector,
        content: Option<String>,
    ) -> Result<EditOperation> {
        Ok(EditOperation::InsertBefore {
            target: selector,
            content,
        })
    }

    fn create_insert_after_operation(
        &self,
        selector: NodeSelector,
        content: Option<String>,
    ) -> Result<EditOperation> {
        Ok(EditOperation::InsertAfter {
            target: selector,
            content,
        })
    }

    fn create_wrap_operation(
        &self,
        selector: NodeSelector,
        content: Option<String>,
    ) -> Result<EditOperation> {
        Ok(EditOperation::Wrap {
            target: selector,
            wrapper_template: content,
        })
    }
}
