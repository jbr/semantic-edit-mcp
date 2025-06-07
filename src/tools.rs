use crate::server::{Tool, ToolCallParams};
use crate::operations::{EditOperation, NodeSelector};
use crate::parsers::{detect_language_from_path, TreeSitterParser};
use crate::validation::SyntaxValidator;
use crate::editors::rust::RustEditor;
use anyhow::{Result, anyhow};
use serde_json::{Value, json};

pub struct ToolRegistry {
    tools: Vec<Tool>,
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
            Tool {
                name: "get_node_info".to_string(),
                description: "Get information about a node at a specific location".to_string(),
                input_schema: serde_json::from_str(include_str!("../schemas/get_node_info.json"))?,
            },
            Tool {
                name: "insert_after_struct".to_string(),
                description: "Insert content after a struct definition (safe structural boundary)".to_string(),
                input_schema: serde_json::from_str(include_str!(
                    "../schemas/insert_after_struct.json"
                ))?,
            },
            Tool {
                name: "insert_after_enum".to_string(),
                description: "Insert content after an enum definition (safe structural boundary)".to_string(),
                input_schema: serde_json::from_str(include_str!(
                    "../schemas/insert_after_enum.json"
                ))?,
            },
            Tool {
                name: "insert_after_impl".to_string(),
                description: "Insert content after an impl block (safe structural boundary)".to_string(),
                input_schema: serde_json::from_str(include_str!(
                    "../schemas/insert_after_impl.json"
                ))?,
            },
            Tool {
                name: "insert_after_function".to_string(),
                description: "Insert content after a function definition (safe structural boundary)".to_string(),
                input_schema: serde_json::from_str(include_str!(
                    "../schemas/insert_after_function.json"
                ))?,
            },
            Tool {
                name: "insert_in_module".to_string(),
                description: "Insert content at module level (top-level items)".to_string(),
                input_schema: serde_json::from_str(include_str!(
                    "../schemas/insert_in_module.json"
                ))?,
            },
        ];

        Ok(Self { tools })
    }

    pub fn get_tools(&self) -> Vec<Tool> {
        self.tools.clone()
    }

        pub async fn execute_tool(&self, tool_call: &ToolCallParams) -> Result<String> {
        let empty_args = json!({});
        let args = tool_call.arguments.as_ref().unwrap_or(&empty_args);

        match tool_call.name.as_str() {
            "replace_node" => self.replace_node(args).await,
            "insert_before_node" => self.insert_before_node(args).await,
            "insert_after_node" => self.insert_after_node(args).await,
            "wrap_node" => self.wrap_node(args).await,
            "validate_syntax" => self.validate_syntax(args).await,
            "validate_edit_context" => self.validate_edit_context(args).await,
            "get_node_info" => self.get_node_info(args).await,
            "insert_after_struct" => self.insert_after_struct(args).await,
            "insert_after_enum" => self.insert_after_enum(args).await,
            "insert_after_impl" => self.insert_after_impl(args).await,
            "insert_after_function" => self.insert_after_function(args).await,
            "insert_in_module" => self.insert_in_module(args).await,
            _ => Err(anyhow!("Unknown tool: {}", tool_call.name)),
        }
    }
}

// Core tool implementations
impl ToolRegistry {
        async fn replace_node(&self, args: &Value) -> Result<String> {
        let file_path = args
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("file_path is required"))?;

        let new_content = args
            .get("new_content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("new_content is required"))?;

        let selector = Self::parse_selector(args.get("selector"))?;
        let preview_only = args
            .get("preview_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let source_code = std::fs::read_to_string(file_path)?;

        let language = detect_language_from_path(file_path)
            .ok_or_else(|| anyhow!("Unable to detect language from file path"))?;

        let mut parser = TreeSitterParser::new()?;
        let tree = parser.parse(&language, &source_code)?;

        // Find the target node
        let target_node = selector
            .find_node_with_suggestions(&tree, &source_code, &language)?
            .ok_or_else(|| anyhow!("Target node not found"))?;

        // NEW: Context validation using tree-sitter queries
        let validator = crate::validation::ContextValidator::new()?;
        let validation_result = validator.validate_insertion(
            &tree, 
            &source_code, 
            &target_node, 
            new_content, 
            &language, 
            &crate::validation::OperationType::Replace
        )?;

        if !validation_result.is_valid {
            let prefix = if preview_only { "PREVIEW: " } else { "" };
            return Ok(format!("{}{}", prefix, validation_result.format_errors()));
        }

        // Continue with existing logic if validation passes
        let operation = EditOperation::Replace {
            target: selector,
            new_content: new_content.to_string(),
            preview_only: Some(preview_only),
        };

        let result = operation.apply(&source_code, &language)?;

        if result.success && !preview_only {
            if let Some(new_code) = &result.new_content {
                std::fs::write(file_path, new_code)?;
            }
        }

        let prefix = if preview_only { "PREVIEW: " } else { "" };
        Ok(format!(
            "{prefix}Replace operation result:\n{}",
            result.message
        ))
    }

    async fn insert_before_node(&self, args: &Value) -> Result<String> {
        let file_path = args
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("file_path is required"))?;

        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("content is required"))?;

        let selector = Self::parse_selector(args.get("selector"))?;
        let preview_only = args
            .get("preview_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let source_code = std::fs::read_to_string(file_path)?;

        let operation = EditOperation::InsertBefore {
            target: selector,
            content: content.to_string(),
            preview_only: Some(preview_only),
        };

        let language = detect_language_from_path(file_path)
            .ok_or_else(|| anyhow!("Unable to detect language from file path"))?;

        let result = operation.apply(&source_code, &language)?;

        if result.success && !preview_only {
            if let Some(new_code) = &result.new_content {
                std::fs::write(file_path, new_code)?;
            }
        }

        let prefix = if preview_only { "PREVIEW: " } else { "" };
        Ok(format!(
            "{prefix}Insert before operation result:\n{}",
            result.message
        ))
    }

        async fn insert_after_node(&self, args: &Value) -> Result<String> {
        let file_path = args
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("file_path is required"))?;

        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("content is required"))?;

        let selector = Self::parse_selector(args.get("selector"))?;
        let source_code = std::fs::read_to_string(file_path)?;
        let preview_only = args
            .get("preview_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let language = detect_language_from_path(file_path)
            .ok_or_else(|| anyhow!("Unable to detect language from file path"))?;

        let mut parser = TreeSitterParser::new()?;
        let tree = parser.parse(&language, &source_code)?;

        // Find the target node
        let target_node = selector
            .find_node_with_suggestions(&tree, &source_code, &language)?
            .ok_or_else(|| anyhow!("Target node not found"))?;

        // NEW: Context validation using tree-sitter queries
        let validator = crate::validation::ContextValidator::new()?;
        let validation_result = validator.validate_insertion(
            &tree, 
            &source_code, 
            &target_node, 
            content, 
            &language, 
            &crate::validation::OperationType::InsertAfter
        )?;

        if !validation_result.is_valid {
            let prefix = if preview_only { "PREVIEW: " } else { "" };
            return Ok(format!("{}{}", prefix, validation_result.format_errors()));
        }

        // Continue with existing logic if validation passes
        let operation = EditOperation::InsertAfter {
            target: selector,
            content: content.to_string(),
            preview_only: Some(preview_only),
        };

        let result = operation.apply(&source_code, &language)?;

        if result.success && !preview_only {
            if let Some(new_code) = &result.new_content {
                std::fs::write(file_path, new_code)?;
            }
        }

        let prefix = if preview_only { "PREVIEW: " } else { "" };
        Ok(format!(
            "{prefix}Insert after operation result:\n{}",
            result.message
        ))
    }

    async fn wrap_node(&self, args: &Value) -> Result<String> {
        let file_path = args
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("file_path is required"))?;

        let wrapper_template = args
            .get("wrapper_template")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("wrapper_template is required"))?;

        let selector = Self::parse_selector(args.get("selector"))?;
        let source_code = std::fs::read_to_string(file_path)?;
        let preview_only = args
            .get("preview_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let operation = EditOperation::Wrap {
            target: selector,
            wrapper_template: wrapper_template.to_string(),
            preview_only: Some(preview_only),
        };

        let language = detect_language_from_path(file_path)
            .ok_or_else(|| anyhow!("Unable to detect language from file path"))?;

        let result = operation.apply(&source_code, &language)?;

        if result.success && !preview_only {
            if let Some(new_code) = &result.new_content {
                std::fs::write(file_path, new_code)?;
            }
        }
        let prefix = if preview_only { "PREVIEW: " } else { "" };
        Ok(format!(
            "{prefix}Wrap operation result:\n{}",
            result.message
        ))
    }

    async fn validate_syntax(&self, args: &Value) -> Result<String> {
        if let Some(file_path) = args.get("file_path").and_then(|v| v.as_str()) {
            let result = SyntaxValidator::validate_file(file_path)?;
            Ok(result.to_string())
        } else if let Some(content) = args.get("content").and_then(|v| v.as_str()) {
            let language = args
                .get("language")
                .and_then(|v| v.as_str())
                .unwrap_or("rust");
            let result = SyntaxValidator::validate_content(content, language)?;
            Ok(result.to_string())
        } else {
            Err(anyhow!("Either file_path or content must be provided"))
        }
    }

    async fn get_node_info(&self, args: &Value) -> Result<String> {
        let file_path = args
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("file_path is required"))?;

        let selector = Self::parse_selector(args.get("selector"))?;
        let source_code = std::fs::read_to_string(file_path)?;

        let language = detect_language_from_path(file_path)
            .ok_or_else(|| anyhow!("Unable to detect language from file path"))?;

        let mut parser = TreeSitterParser::new()?;
        let tree = parser.parse(&language, &source_code)?;

        match language.as_str() {
            "rust" => RustEditor::get_node_info(&tree, &source_code, &selector),
            _ => Err(anyhow!("Unsupported language for node info: {}", language)),
        }
    }
    
    async fn validate_edit_context(&self, args: &Value) -> Result<String> {
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

        let selector = Self::parse_selector(args.get("selector"))?;
        let source_code = std::fs::read_to_string(file_path)?;

        let language = detect_language_from_path(file_path)
            .ok_or_else(|| anyhow!("Unable to detect language from file path"))?;

        let mut parser = TreeSitterParser::new()?;
        let tree = parser.parse(&language, &source_code)?;

        // Find the target node
        let target_node = selector
            .find_node_with_suggestions(&tree, &source_code, &language)?
            .ok_or_else(|| anyhow!("Target node not found"))?;

        // Perform context validation
        let validator = crate::validation::ContextValidator::new()?;
        let validation_result = validator.validate_insertion(
            &tree, 
            &source_code, 
            &target_node, 
            content, 
            &language, 
            &operation_type
        )?;

        if validation_result.is_valid {
            Ok("✅ Edit context validation passed - this placement is semantically valid".to_string())
        } else {
            Ok(validation_result.format_errors())
        }
    }

    fn parse_selector(selector_value: Option<&Value>) -> Result<NodeSelector> {
        let selector_obj = selector_value
            .ok_or_else(|| anyhow!("selector is required"))?
            .as_object()
            .ok_or_else(|| anyhow!("selector must be an object"))?;

        if let (Some(line), Some(column)) = (
            selector_obj.get("line").and_then(|v| v.as_u64()),
            selector_obj.get("column").and_then(|v| v.as_u64()),
        ) {
            let scope = selector_obj
                .get("scope")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());
            return Ok(NodeSelector::Position {
                line: line as usize,
                column: column as usize,
                scope,
            });
        }

        if let Some(query) = selector_obj.get("query").and_then(|v| v.as_str()) {
            return Ok(NodeSelector::Query {
                query: query.to_string(),
            });
        }

        if let Some(node_type) = selector_obj.get("type").and_then(|v| v.as_str()) {
            if let Some(name) = selector_obj.get("name").and_then(|v| v.as_str()) {
                return Ok(NodeSelector::Name {
                    node_type: Some(node_type.to_string()),
                    name: name.to_string(),
                });
            } else {
                return Ok(NodeSelector::Type {
                    node_type: node_type.to_string(),
                });
            }
        }

        if let Some(name) = selector_obj.get("name").and_then(|v| v.as_str()) {
            return Ok(NodeSelector::Name {
                node_type: None,
                name: name.to_string(),
            });
        }

        Err(anyhow!(
            "Invalid selector: must specify position, query, type, or name"
        ))
    }
}
