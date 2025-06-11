use crate::operations::{EditOperation, NodeSelector};
use crate::parsers::{detect_language_from_path, TreeSitterParser};
use crate::server::{Tool, ToolCallParams};
use crate::validation::SyntaxValidator;
use anyhow::{anyhow, Result};
use serde_json::{json, Value};

pub struct ToolRegistry {
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
                name: "replace_node".to_string(),
                description: "Replace an entire AST node with new content".to_string(),
                input_schema: serde_json::from_str(include_str!("../schemas/replace_node.json"))?,
            },
            Tool {
                name: "insert_before_node".to_string(),
                description: "Insert content before a specified AST node".to_string(),
                input_schema: serde_json::from_str(include_str!(
                    "../schemas/insert_before_node.json"
                ))?,
            },
            Tool {
                name: "insert_after_node".to_string(),
                description: "Insert content after a specified AST node".to_string(),
                input_schema: serde_json::from_str(include_str!(
                    "../schemas/insert_after_node.json"
                ))?,
            },
            Tool {
                name: "wrap_node".to_string(),
                description: "Wrap an AST node with new syntax".to_string(),
                input_schema: serde_json::from_str(include_str!("../schemas/wrap_node.json"))?,
            },
            Tool {
                name: "validate_syntax".to_string(),
                description: "Validate that a file or code snippet has correct syntax".to_string(),
                input_schema: serde_json::from_str(include_str!(
                    "../schemas/validate_syntax.json"
                ))?,
            },
        ];

        Ok(Self { tools })
    }

    pub fn get_tools(&self) -> Vec<Tool> {
        self.tools.clone()
    }

    pub async fn execute_tool(&self, tool_call: &ToolCallParams) -> Result<ExecutionResult> {
        let empty_args = json!({});
        let args = tool_call.arguments.as_ref().unwrap_or(&empty_args);

        match tool_call.name.as_str() {
            "replace_node" => self.replace_node(args).await,
            "insert_before_node" => self.insert_before_node(args).await,
            "insert_after_node" => self.insert_after_node(args).await,
            "wrap_node" => self.wrap_node(args).await,
            "validate_syntax" => self.validate_syntax(args).await,
            "validate_edit_context" => self.validate_edit_context(args).await,
            _ => Err(anyhow!("Unknown tool: {}", tool_call.name)),
        }
    }
}

// Core tool implementations
impl ToolRegistry {
    async fn replace_node(&self, args: &Value) -> Result<ExecutionResult> {
        let file_path = args
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("file_path is required"))?;

        let new_content = args
            .get("new_content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("new_content is required"))?;
        let selector = Self::parse_selector(args.get("selector"), false)?; // Disallow position for edits
        let preview_only = args
            .get("preview_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let language_hint = args
            .get("language")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let operation = EditOperation::Replace {
            target: selector,
            new_content: new_content.to_string(),
            preview_only: Some(preview_only),
        };

        operation.apply_with_validation(language_hint, file_path, preview_only)
    }

    async fn insert_before_node(&self, args: &Value) -> Result<ExecutionResult> {
        let file_path = args
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("file_path is required"))?;

        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("content is required"))?;
        let selector = Self::parse_selector(args.get("selector"), false)?; // Disallow position for edits
        let preview_only = args
            .get("preview_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let language_hint = args
            .get("language")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let operation = EditOperation::InsertBefore {
            target: selector,
            content: content.to_string(),
            preview_only: Some(preview_only),
        };

        operation.apply_with_validation(language_hint, file_path, preview_only)
    }

    async fn insert_after_node(&self, args: &Value) -> Result<ExecutionResult> {
        let file_path = args
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("file_path is required"))?;
        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("content is required"))?;
        let selector = Self::parse_selector(args.get("selector"), false)?; // Disallow position for edits
        let preview_only = args
            .get("preview_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let language_hint = args
            .get("language")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        let operation = EditOperation::InsertAfter {
            target: selector,
            content: content.to_string(),
            preview_only: Some(preview_only),
        };
        operation.apply_with_validation(language_hint, file_path, preview_only)
    }

    async fn wrap_node(&self, args: &Value) -> Result<ExecutionResult> {
        let file_path = args
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("file_path is required"))?;
        let wrapper_template = args
            .get("wrapper_template")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("wrapper_template is required"))?;
        let selector = Self::parse_selector(args.get("selector"), false)?; // Disallow position for edits
        let preview_only = args
            .get("preview_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let language_hint = args
            .get("language")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());

        let operation = EditOperation::Wrap {
            target: selector,
            wrapper_template: wrapper_template.to_string(),
            preview_only: Some(preview_only),
        };

        operation.apply_with_validation(language_hint, file_path, preview_only)
    }

    async fn validate_syntax(&self, args: &Value) -> Result<ExecutionResult> {
        if let Some(file_path) = args.get("file_path").and_then(|v| v.as_str()) {
            let result = SyntaxValidator::validate_file(file_path)?;
            Ok(ExecutionResult::ResponseOnly(result.to_string()))
        } else if let Some(content) = args.get("content").and_then(|v| v.as_str()) {
            let language = args
                .get("language")
                .and_then(|v| v.as_str())
                .unwrap_or("rust");
            let result = SyntaxValidator::validate_content(content, language)?;
            Ok(ExecutionResult::ResponseOnly(result.to_string()))
        } else {
            Err(anyhow!("Either file_path or content must be provided"))
        }
    }

    

    async fn validate_edit_context(&self, args: &Value) -> Result<ExecutionResult> {
        let file_path = args
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("file_path is required"))?;

        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("content is required"))?;

        let operation_type_str = args
            .get("operation_type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("operation_type is required"))?;

        let operation_type = match operation_type_str {
            "insert_before" => crate::validation::OperationType::InsertBefore,
            "insert_after" => crate::validation::OperationType::InsertAfter,
            "replace" => crate::validation::OperationType::Replace,
            "wrap" => crate::validation::OperationType::Wrap,
            _ => return Err(anyhow!("Invalid operation_type: {}", operation_type_str)),
        };

        let selector = Self::parse_selector(args.get("selector"), false)?;
        let source_code = std::fs::read_to_string(file_path)?;

        let language = detect_language_from_path(file_path)
            .ok_or_else(|| anyhow!("Unable to detect language from file path"))?;

        let mut parser = TreeSitterParser::new()?;
        let tree = parser.parse(&language, &source_code)?;

        // Find the target node
        let target_node = selector
            .find_node_with_suggestions(&tree, &source_code, &language)?
            .ok_or_else(|| anyhow!("Target node not found"))?;

        // Check for terrible targets with auto-exploration
        if let Some(error_msg) = crate::operations::validation::check_terrible_target(
            &selector,
            &target_node,
            &tree,
            &source_code,
            &language,
        )? {
            return Ok(ExecutionResult::ResponseOnly(error_msg));
        }

        // Perform context validation
        let validator = crate::validation::ContextValidator::new()?;
        if !validator.supports_language(&language) {
            return Ok(ExecutionResult::ResponseOnly(format!(
                "ℹ️ Context validation is not available for {language} files. Only syntax validation is supported for this language.",
            )));
        }
        let validation_result = validator.validate_insertion(
            &tree,
            &source_code,
            &target_node,
            content,
            &language,
            &operation_type,
        )?;

        if validation_result.is_valid {
            Ok(ExecutionResult::ResponseOnly(
                "✅ Edit context validation passed - this placement is semantically valid"
                    .to_string(),
            ))
        } else {
            Ok(ExecutionResult::ResponseOnly(
                validation_result.format_errors(),
            ))
        }
    }

    fn parse_selector(
        selector_value: Option<&Value>,
        allow_position: bool,
    ) -> Result<NodeSelector> {
        let selector_obj = selector_value
            .ok_or_else(|| anyhow!("selector is required"))?
            .as_object()
            .ok_or_else(|| anyhow!("selector must be an object"))?;

        // Handle position-based selectors (only for exploration tools like get_node_info)
        if let (Some(line), Some(column)) = (
            selector_obj.get("line").and_then(|v| v.as_u64()),
            selector_obj.get("column").and_then(|v| v.as_u64()),
        ) {
            if !allow_position {
                return Err(anyhow!(
                    "Position-based targeting (line/column) is not allowed for edit operations.\n\
                     Use text-anchored selectors instead:\n\
                     • {{\"anchor_text\": \"exact text to find\", \"ancestor_node_type\": \"function_item\"}}\n\
                     \n\
                     💡 Use explore_ast with line/column to find the right anchor text and node type."
                ));
            }

            // For position-based selectors in exploration tools, we need to convert them
            // to a temporary format. Since the new NodeSelector doesn't support position,
            // we'll handle this case differently in the calling code.
            return Err(anyhow!("Position-based selectors need special handling - this should be implemented in the calling function"));
        }

        // Handle text-anchored selectors
        if let (Some(anchor_text), Some(ancestor_node_type)) = (
            selector_obj.get("anchor_text").and_then(|v| v.as_str()),
            selector_obj
                .get("ancestor_node_type")
                .and_then(|v| v.as_str()),
        ) {
            return Ok(NodeSelector {
                anchor_text: anchor_text.to_string(),
                ancestor_node_type: ancestor_node_type.to_string(),
            });
        }

        Err(anyhow!(
            "Invalid selector: must specify either:\n\
             • Text-anchored: {{\"anchor_text\": \"exact text\", \"ancestor_node_type\": \"node_type\"}}\n\
             • Position (exploration only): {{\"line\": N, \"column\": N}}"
        ))
    }

    
}
