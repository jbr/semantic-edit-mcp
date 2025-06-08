use crate::editors::rust::RustEditor;
use crate::operations::{check_terrible_target, EditOperation, NodeSelector};
use crate::parsers::{detect_language_from_path, TreeSitterParser};
use crate::server::{Tool, ToolCallParams};
use crate::validation::SyntaxValidator;
use anyhow::{anyhow, Result};
use serde_json::{json, Value};

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
                description: "Insert content after a struct definition (safe structural boundary)"
                    .to_string(),
                input_schema: serde_json::from_str(include_str!(
                    "../schemas/insert_after_struct.json"
                ))?,
            },
            Tool {
                name: "insert_after_enum".to_string(),
                description: "Insert content after an enum definition (safe structural boundary)"
                    .to_string(),
                input_schema: serde_json::from_str(include_str!(
                    "../schemas/insert_after_enum.json"
                ))?,
            },
            Tool {
                name: "insert_after_impl".to_string(),
                description: "Insert content after an impl block (safe structural boundary)"
                    .to_string(),
                input_schema: serde_json::from_str(include_str!(
                    "../schemas/insert_after_impl.json"
                ))?,
            },
            Tool {
                name: "insert_after_function".to_string(),
                description:
                    "Insert content after a function definition (safe structural boundary)"
                        .to_string(),
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
            Tool {
                name: "explore_ast".to_string(),
                description: "Explore the AST around a specific position with rich context and edit suggestions".to_string(),
                input_schema: serde_json::from_str(include_str!(
                    "../schemas/explore_ast.json"
                ))?
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
            "explore_ast" => self.explore_ast(args).await,
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

        // Try language hint first, then fall back to auto-detection
        let language = args
            .get("language")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| detect_language_from_path(file_path))
            .ok_or_else(|| {
                anyhow!("Unable to detect language from file path and no language hint provided")
            })?;

        // For new multi-language support, use the language registry
        if let Ok(registry) = crate::languages::LanguageRegistry::new() {
            if let Some(lang_support) = registry.get_language(&language) {
                // Use the new language-specific editor
                let mut parser = tree_sitter::Parser::new();
                parser.set_language(&lang_support.tree_sitter_language())?;
                let tree = parser
                    .parse(&source_code, None)
                    .ok_or_else(|| anyhow!("Failed to parse {} code", language))?;

                let editor = lang_support.editor();
                return editor.get_node_info(&tree, &source_code, &selector);
            }
        }

        // Fallback to old Rust-only logic
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

        // Check for terrible targets with auto-exploration
        if let Some(error_msg) =
            check_terrible_target(&selector, &target_node, &tree, &source_code, &language)?
        {
            return Ok(error_msg);
        }

        // Perform context validation
        let validator = crate::validation::ContextValidator::new()?;
        if !validator.supports_language(&language) {
            return Ok(format!(
                "ℹ️ Context validation is not available for {language} files. Only syntax validation is supported for this language.",
            ));
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
            Ok(
                "✅ Edit context validation passed - this placement is semantically valid"
                    .to_string(),
            )
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

    async fn explore_ast(&self, args: &Value) -> Result<String> {
        let file_path = args
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("file_path is required"))?;

        let line = args
            .get("line")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow!("line is required"))? as usize;

        let column = args
            .get("column")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| anyhow!("column is required"))? as usize;

        let source_code = std::fs::read_to_string(file_path)?;

        // Try language hint first, then fall back to auto-detection
        let language = args
            .get("language")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string())
            .or_else(|| detect_language_from_path(file_path))
            .ok_or_else(|| {
                anyhow!("Unable to detect language from file path and no language hint provided")
            })?;

        let mut parser = TreeSitterParser::new()?;
        let tree = parser.parse(&language, &source_code)?;

        // Use the AST explorer
        let exploration_result = crate::ast_explorer::ASTExplorer::explore_around(
            &tree,
            &source_code,
            line,
            column,
            &language,
        )?;

        // Format the results in a nice, readable way
        let mut output = String::new();
        output.push_str(&format!("🔍 AST Exploration at {line}:{column}\n\n"));

        // Show the focus node
        output.push_str(&format!(
            "🎯 **Focus Node**: {} (ID: {})\n",
            exploration_result.focus_node.kind, exploration_result.focus_node.id
        ));
        if let Some(role) = &exploration_result.focus_node.semantic_role {
            output.push_str(&format!("   Role: {role}\n"));
        }
        output.push_str(&format!(
            "   Content: \"{}\"\n",
            exploration_result.focus_node.text_preview
        ));
        output.push_str(&format!(
            "   Position: lines {}-{}, chars {}-{}\n\n",
            exploration_result.focus_node.line_range.0,
            exploration_result.focus_node.line_range.1,
            exploration_result.focus_node.char_range.0,
            exploration_result.focus_node.char_range.1
        ));

        // Show selector options
        output.push_str("🎯 **Available Selectors**:\n");
        for (i, selector) in exploration_result
            .focus_node
            .selector_options
            .iter()
            .enumerate()
        {
            output.push_str(&format!(
                "  {}. {} (confidence: {:.0}%)\n",
                i + 1,
                selector.description,
                selector.confidence * 100.0
            ));
            output.push_str(&format!(
                "     Selector: {}\n",
                serde_json::to_string_pretty(&selector.selector_value)?
            ));
        }
        output.push('\n');

        // Show context hierarchy (ancestors)
        if !exploration_result.ancestors.is_empty() {
            output.push_str("📍 **Context Hierarchy** (inner → outer):\n");
            for (i, ancestor) in exploration_result.ancestors.iter().enumerate() {
                let indent = "  ".repeat(i + 1);
                output.push_str(&format!(
                    "{}{}. {} (ID: {})",
                    indent,
                    i + 1,
                    ancestor.kind,
                    ancestor.id
                ));
                if let Some(role) = &ancestor.semantic_role {
                    output.push_str(&format!(" - {role}"));
                }
                output.push('\n');
                if !ancestor.text_preview.is_empty() && ancestor.text_preview.len() < 60 {
                    output.push_str(&format!(
                        "{}   Content: \"{}\"\n",
                        indent, ancestor.text_preview
                    ));
                }
            }
            output.push('\n');
        }

        // Show children
        if !exploration_result.children.is_empty() {
            output.push_str("👶 **Child Nodes**:\n");
            for (i, child) in exploration_result.children.iter().take(10).enumerate() {
                output.push_str(&format!("  {}. {} (ID: {})", i + 1, child.kind, child.id));
                if let Some(role) = &child.semantic_role {
                    output.push_str(&format!(" - {role}"));
                }
                output.push('\n');
            }
            if exploration_result.children.len() > 10 {
                output.push_str(&format!(
                    "  ... and {} more children\n",
                    exploration_result.children.len() - 10
                ));
            }
            output.push('\n');
        }

        // Show siblings
        if !exploration_result.siblings.is_empty() {
            output.push_str("👫 **Sibling Nodes**:\n");
            for (i, sibling) in exploration_result.siblings.iter().take(8).enumerate() {
                output.push_str(&format!(
                    "  {}. {} (ID: {})",
                    i + 1,
                    sibling.kind,
                    sibling.id
                ));
                if let Some(role) = &sibling.semantic_role {
                    output.push_str(&format!(" - {role}"));
                }
                output.push('\n');
            }
            if exploration_result.siblings.len() > 8 {
                output.push_str(&format!(
                    "  ... and {} more siblings\n",
                    exploration_result.siblings.len() - 8
                ));
            }
            output.push('\n');
        }

        // Show edit recommendations
        if !exploration_result.edit_recommendations.is_empty() {
            output.push_str("💡 **Edit Recommendations**:\n");
            for (i, rec) in exploration_result.edit_recommendations.iter().enumerate() {
                output.push_str(&format!(
                    "  {}. {} (confidence: {:.0}%)\n",
                    i + 1,
                    rec.description,
                    rec.confidence * 100.0
                ));
                output.push_str(&format!("     Operation: {}\n", rec.operation));
                output.push_str(&format!("     Example: {}\n", rec.example_usage));
            }
            output.push('\n');
        }

        // Show context explanation
        output.push_str("📖 **Context Analysis**:\n");
        output.push_str(&exploration_result.context_explanation);

        Ok(output)
    }
}
