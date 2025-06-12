use crate::languages::LanguageRegistry;
use crate::operations::{EditOperation, NodeSelector};
use crate::server::{Tool, ToolCallParams};
use anyhow::{anyhow, Result};
use serde_json::{json, Value};

pub struct ToolRegistry {
    language_registry: LanguageRegistry,
    tools: Vec<Tool>,
}

pub enum ExecutionResult {
    ResponseOnly(String),
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
                description:
                    "Read the docs for this tool. Do this once for each language used per session.",
                input_schema: serde_json::from_str(include_str!(
                    "../schemas/read_documentation.json"
                ))?,
            },
            Tool {
                name: "replace_node",
                description: "Replace an entire AST node with new content",
                input_schema: serde_json::from_str(include_str!("../schemas/replace_node.json"))?,
            },
            Tool {
                name: "remove_node",
                description: "Remove an entire AST node",
                input_schema: serde_json::from_str(include_str!("../schemas/remove_node.json"))?,
            },
            Tool {
                name: "insert_before_node",
                description: "Insert content before a specified AST node",
                input_schema: serde_json::from_str(include_str!(
                    "../schemas/insert_before_node.json"
                ))?,
            },
            Tool {
                name: "insert_after_node",
                description: "Insert content after a specified AST node",
                input_schema: serde_json::from_str(include_str!(
                    "../schemas/insert_after_node.json"
                ))?,
            },
            Tool {
                name: "wrap_node",
                description: "Wrap an AST node with new syntax",
                input_schema: serde_json::from_str(include_str!("../schemas/wrap_node.json"))?,
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

    pub async fn execute_tool(&self, tool_call: &ToolCallParams) -> Result<ExecutionResult> {
        let empty_args = json!({});
        let args = tool_call.arguments.as_ref().unwrap_or(&empty_args);

        if tool_call.name == "read_documentation" {
            let language = args
                .get("language")
                .and_then(|v| v.as_str())
                .ok_or_else(|| anyhow!("language is required"))?;

            return Ok(ExecutionResult::ResponseOnly(
                self.language_registry.get_documentation(language)?,
            ));
        }

        let file_path = args
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("file_path is required"))?;
        let preview_only = args
            .get("preview_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let language_hint = args
            .get("language")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let selector = NodeSelector::new_from_value(args)?;

        let operation = match tool_call.name.as_str() {
            "replace_node" => self.replace_node(selector, preview_only, args),
            "remove_node" => self.remove_node(selector, preview_only, args),
            "insert_before_node" => self.insert_before_node(selector, preview_only, args),
            "insert_after_node" => self.insert_after_node(selector, preview_only, args),
            "wrap_node" => self.wrap_node(selector, preview_only, args),
            _ => Err(anyhow!("Unknown tool: {}", tool_call.name)),
        }?;

        let language = self
            .language_registry
            .get_language_with_hint(file_path, language_hint.as_deref())?;

        operation.apply(language, file_path, preview_only)
    }

    fn remove_node(
        &self,
        selector: NodeSelector,
        preview_only: bool,
        _args: &Value,
    ) -> Result<EditOperation> {
        Ok(EditOperation::Delete {
            target: selector,
            preview_only,
        })
    }

    fn replace_node(
        &self,
        selector: NodeSelector,
        preview_only: bool,
        args: &Value,
    ) -> Result<EditOperation> {
        let new_content = args
            .get("new_content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("new_content is required"))?;
        Ok(EditOperation::Replace {
            target: selector,
            new_content: new_content.to_string(),
            preview_only,
        })
    }

    fn insert_before_node(
        &self,
        selector: NodeSelector,
        preview_only: bool,
        args: &Value,
    ) -> Result<EditOperation> {
        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("content is required"))?;
        Ok(EditOperation::InsertBefore {
            target: selector,
            content: content.to_string(),
            preview_only,
        })
    }

    fn insert_after_node(
        &self,
        selector: NodeSelector,
        preview_only: bool,
        args: &Value,
    ) -> Result<EditOperation> {
        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("content is required"))?;
        Ok(EditOperation::InsertAfter {
            target: selector,
            content: content.to_string(),
            preview_only,
        })
    }

    fn wrap_node(
        &self,
        selector: NodeSelector,
        preview_only: bool,
        args: &Value,
    ) -> Result<EditOperation> {
        let wrapper_template = args
            .get("wrapper_template")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("wrapper_template is required"))?;
        Ok(EditOperation::Wrap {
            target: selector,
            wrapper_template: wrapper_template.to_string(),
            preview_only,
        })
    }
}
