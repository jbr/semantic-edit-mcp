use crate::tools::ToolRegistry;
use crate::operations::{EditOperation, NodeSelector};
use crate::parsers::{detect_language_from_path, TreeSitterParser};
use anyhow::{Result, anyhow};
use serde_json::Value;

// Specialized insertion tools implementations
impl ToolRegistry {
    pub async fn insert_after_struct(&self, args: &Value) -> Result<String> {
        let file_path = args
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("file_path is required"))?;

        let struct_name = args
            .get("struct_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("struct_name is required"))?;

        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("content is required"))?;

        let preview_only = args
            .get("preview_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let source_code = std::fs::read_to_string(file_path)?;

        let selector = NodeSelector::Name {
            node_type: Some("struct_item".to_string()),
            name: struct_name.to_string(),
        };

        let operation = EditOperation::InsertAfter {
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
            "{prefix}Insert after struct operation result:\n{}",
            result.message
        ))
    }

    pub async fn insert_after_enum(&self, args: &Value) -> Result<String> {
        let file_path = args
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("file_path is required"))?;

        let enum_name = args
            .get("enum_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("enum_name is required"))?;

        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("content is required"))?;

        let preview_only = args
            .get("preview_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let source_code = std::fs::read_to_string(file_path)?;

        let selector = NodeSelector::Name {
            node_type: Some("enum_item".to_string()),
            name: enum_name.to_string(),
        };

        let operation = EditOperation::InsertAfter {
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
            "{prefix}Insert after enum operation result:\n{}",
            result.message
        ))
    }

    pub async fn insert_after_impl(&self, args: &Value) -> Result<String> {
        let file_path = args
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("file_path is required"))?;

        let impl_type = args
            .get("impl_type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("impl_type is required"))?;

        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("content is required"))?;

        let preview_only = args
            .get("preview_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let source_code = std::fs::read_to_string(file_path)?;

        // For impl blocks, we need to find them by their type
        let selector = NodeSelector::Query {
            query: format!(
                r#"
                (impl_item
                    type: (type_identifier) @type_name
                    (#eq? @type_name "{impl_type}")) @impl
                "#
            ),
        };

        let operation = EditOperation::InsertAfter {
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
            "{prefix}Insert after impl operation result:\n{}",
            result.message
        ))
    }

    pub async fn insert_after_function(&self, args: &Value) -> Result<String> {
        let file_path = args
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("file_path is required"))?;

        let function_name = args
            .get("function_name")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("function_name is required"))?;

        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("content is required"))?;

        let preview_only = args
            .get("preview_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let source_code = std::fs::read_to_string(file_path)?;

        let selector = NodeSelector::Name {
            node_type: Some("function_item".to_string()),
            name: function_name.to_string(),
        };

        let operation = EditOperation::InsertAfter {
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
            "{prefix}Insert after function operation result:\n{}",
            result.message
        ))
    }

        pub async fn insert_in_module(&self, args: &Value) -> Result<String> {
        let file_path = args
            .get("file_path")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("file_path is required"))?;

        let content = args
            .get("content")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("content is required"))?;

        let preview_only = args
            .get("preview_only")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);

        let position = args
            .get("position")
            .and_then(|v| v.as_str())
            .unwrap_or("end"); // "start" or "end"

        let source_code = std::fs::read_to_string(file_path)?;
        let mut parser = TreeSitterParser::new()?;
        let tree = parser.parse("rust", &source_code)?;

        let operation = if position == "start" {
            // Strategy for "start": Insert after the last use statement, or after any top-level attributes/comments
            
            // First, try to find the last use statement
            let use_query = NodeSelector::Query {
                query: r#"
                    (source_file (use_declaration) @use)
                "#.to_string(),
            };
            
            // Get all use statements and find the last one
            let language_obj = tree_sitter_rust::LANGUAGE.into();
            let query = tree_sitter::Query::new(&language_obj, "(source_file (use_declaration) @use)")?;
            let mut cursor = tree_sitter::QueryCursor::new();
            let mut last_use_node = None;
            
            for m in cursor.matches(&query, tree.root_node(), source_code.as_bytes()) {
                for capture in m.captures {
                    if capture.index == query.capture_index_for_name("use").unwrap() {
                        last_use_node = Some(capture.node);
                    }
                }
            }
            
            if let Some(use_node) = last_use_node {
                // Insert after the last use statement
                EditOperation::InsertAfter {
                    target: NodeSelector::Query {
                        query: format!(
                            r#"(use_declaration) @target"#
                        ),
                    },
                    content: format!("\n{content}"),
                    preview_only: Some(preview_only),
                }
            } else {
                // No use statements found, find the first actual item (function, struct, etc.)
                let first_item_query = r#"
                    (source_file 
                        [
                            (function_item) @item
                            (struct_item) @item
                            (enum_item) @item
                            (impl_item) @item
                            (mod_item) @item
                            (const_item) @item
                            (static_item) @item
                            (type_item) @item
                            (trait_item) @item
                        ])
                "#;
                
                let selector = NodeSelector::Query {
                    query: first_item_query.to_string(),
                };
                
                match selector.find_node(&tree, &source_code, "rust")? {
                    Some(_) => EditOperation::InsertBefore {
                        target: selector,
                        content: format!("{content}\n\n"),
                        preview_only: Some(preview_only),
                    },
                    None => {
                        // Empty file or only comments, just append at the end
                        EditOperation::InsertAfter {
                            target: NodeSelector::Query {
                                query: "(source_file) @root".to_string(),
                            },
                            content: content.to_string(),
                            preview_only: Some(preview_only),
                        }
                    }
                }
            }
        } else {
            // Strategy for "end": Insert after the last top-level item
            let last_item_query = r#"
                (source_file 
                    [
                        (function_item) @item
                        (struct_item) @item
                        (enum_item) @item
                        (impl_item) @item
                        (mod_item) @item
                        (const_item) @item
                        (static_item) @item
                        (type_item) @item
                        (trait_item) @item
                        (use_declaration) @item
                    ])
            "#;
            
            // Find all top-level items and get the last one
            let language_obj = tree_sitter_rust::LANGUAGE.into();
            let query = tree_sitter::Query::new(&language_obj, last_item_query)?;
            let mut cursor = tree_sitter::QueryCursor::new();
            let mut last_item_node = None;
            
            for m in cursor.matches(&query, tree.root_node(), source_code.as_bytes()) {
                for capture in m.captures {
                    if capture.index == query.capture_index_for_name("item").unwrap() {
                        last_item_node = Some(capture.node);
                    }
                }
            }
            
            if let Some(last_node) = last_item_node {
                // Insert after the last item by finding its specific type and name
                let node_kind = last_node.kind();
                let node_text = &source_code[last_node.start_byte()..last_node.end_byte()];
                
                // For functions, structs, etc., we can target them specifically
                match node_kind {
                    "function_item" => {
                        // Extract function name for precise targeting
                        if let Some(name_node) = last_node.child_by_field_name("name") {
                            let name = &source_code[name_node.start_byte()..name_node.end_byte()];
                            EditOperation::InsertAfter {
                                target: NodeSelector::Name {
                                    node_type: Some("function_item".to_string()),
                                    name: name.to_string(),
                                },
                                content: format!("\n{content}"),
                                preview_only: Some(preview_only),
                            }
                        } else {
                            // Fallback to generic query
                            EditOperation::InsertAfter {
                                target: NodeSelector::Query {
                                    query: format!("({}) @target", node_kind),
                                },
                                content: format!("\n{content}"),
                                preview_only: Some(preview_only),
                            }
                        }
                    },
                    "struct_item" | "enum_item" => {
                        // Extract type name for precise targeting  
                        if let Some(name_node) = last_node.child_by_field_name("name") {
                            let name = &source_code[name_node.start_byte()..name_node.end_byte()];
                            EditOperation::InsertAfter {
                                target: NodeSelector::Name {
                                    node_type: Some(node_kind.to_string()),
                                    name: name.to_string(),
                                },
                                content: format!("\n{content}"),
                                preview_only: Some(preview_only),
                            }
                        } else {
                            EditOperation::InsertAfter {
                                target: NodeSelector::Query {
                                    query: format!("({}) @target", node_kind),
                                },
                                content: format!("\n{content}"),
                                preview_only: Some(preview_only),
                            }
                        }
                    },
                    _ => {
                        // For other items, use a generic approach
                        EditOperation::InsertAfter {
                            target: NodeSelector::Query {
                                query: format!("({}) @target", node_kind),
                            },
                            content: format!("\n{content}"),
                            preview_only: Some(preview_only),
                        }
                    }
                }
            } else {
                // No items found, file might be empty or only have comments
                // Just append to the end of the file using a simple string append
                let new_content = if source_code.trim().is_empty() {
                    content.to_string()
                } else {
                    format!("{}\n\n{}", source_code.trim_end(), content)
                };
                
                if !preview_only {
                    std::fs::write(file_path, &new_content)?;
                }
                
                let prefix = if preview_only { "PREVIEW: " } else { "" };
                return Ok(format!(
                    "{prefix}Insert in module operation result:\nSuccessfully appended content to end of file"
                ));
            }
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
            "{prefix}Insert in module operation result:\n{}",
            result.message
        ))
    }
}