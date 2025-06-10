use crate::operations::{EditOperation, EditResult, NodeSelector};
use crate::parsers::{get_node_text, TreeSitterParser};
use anyhow::{anyhow, Result};
use ropey::Rope;
use tree_sitter::{Node, Tree};

pub struct RustEditor;

impl RustEditor {
    pub fn apply_operation(operation: &EditOperation, source_code: &str) -> Result<EditResult> {
        let mut parser = TreeSitterParser::new()?;
        let tree = parser.parse("rust", source_code)?;

        match operation {
            EditOperation::Replace {
                target,
                new_content,
                preview_only,
            } => {
                let mut result = Self::replace_node(&tree, source_code, target, new_content)?;
                if preview_only.unwrap_or(false) {
                    result.set_message(format!("PREVIEW: {}", result.message()));
                    // Don't modify the file in preview mode, but show what would happen
                }
                Ok(result)
            }
            EditOperation::InsertBefore {
                target,
                content,
                preview_only,
            } => {
                let mut result = Self::insert_before_node(&tree, source_code, target, content)?;
                if preview_only.unwrap_or(false) {
                    result.set_message(format!("PREVIEW: {}", result.message()));
                }
                Ok(result)
            }
            EditOperation::InsertAfter {
                target,
                content,
                preview_only,
            } => {
                let mut result = Self::insert_after_node(&tree, source_code, target, content)?;
                if preview_only.unwrap_or(false) {
                    result.set_message(format!("PREVIEW: {}", result.message()));
                }
                Ok(result)
            }
            EditOperation::Wrap {
                target,
                wrapper_template,
                preview_only,
            } => {
                let mut result = Self::wrap_node(&tree, source_code, target, wrapper_template)?;
                if preview_only.unwrap_or(false) {
                    result.set_message(format!("PREVIEW: {}", result.message()));
                }
                Ok(result)
            }
            EditOperation::Delete {
                target,
                preview_only,
            } => {
                let mut result = Self::delete_node(&tree, source_code, target)?;
                if preview_only.unwrap_or(false) {
                    result.set_message(format!("PREVIEW: {}", result.message()));
                }
                Ok(result)
            }
        }
    }

    fn replace_node(
        tree: &Tree,
        source_code: &str,
        selector: &NodeSelector,
        new_content: &str,
    ) -> Result<EditResult> {
        let node = selector
            .find_node_with_suggestions(tree, source_code, "rust")?
            .ok_or_else(|| anyhow!("Target node not found"))?;

        // Smart attribute handling for function replacements
        let (actual_start_byte, actual_end_byte) = if node.kind() == "function_item" {
            Self::calculate_function_replacement_range(tree, source_code, &node, new_content)?
        } else {
            (node.start_byte(), node.end_byte())
        };

        // Validate the new content would create valid syntax
        if !Self::validate_replacement_with_range(
            source_code,
            actual_start_byte,
            actual_end_byte,
            new_content,
        )? {
            return Ok(EditResult::Error(
                "Replacement would create invalid syntax".to_string(),
            ));
        }

        let rope = Rope::from_str(source_code);

        // Convert byte positions to character positions
        let start_char = rope.byte_to_char(actual_start_byte);
        let end_char = rope.byte_to_char(actual_end_byte);

        // Create new rope with replacement
        let mut new_rope = rope.clone();
        new_rope.remove(start_char..end_char);
        new_rope.insert(start_char, new_content);

        Ok(EditResult::Success {
            message: format!("Successfully replaced {} node", node.kind()),
            new_content: new_rope.to_string(),
            affected_range: (start_char, start_char + new_content.len()),
        })
    }

    /// Calculate the correct replacement range for functions, including attributes if appropriate
    fn calculate_function_replacement_range(
        tree: &Tree,
        source_code: &str,
        function_node: &Node,
        new_content: &str,
    ) -> Result<(usize, usize)> {
        // Check if the new content starts with attributes
        let new_content_has_attributes = new_content.trim_start().starts_with('#');

        if !new_content_has_attributes {
            // No attributes in replacement, preserve existing ones by only replacing the function
            return Ok((function_node.start_byte(), function_node.end_byte()));
        }

        // New content has attributes, so include any existing attributes in replacement
        // to prevent duplication
        if let Some((attr_start, func_end)) =
            Self::find_function_attributes_range(tree, source_code, function_node)
        {
            Ok((attr_start, func_end))
        } else {
            // No existing attributes found, just replace the function
            Ok((function_node.start_byte(), function_node.end_byte()))
        }
    }

    /// Find the range that includes any attributes immediately preceding a function
    fn find_function_attributes_range(
        tree: &Tree,
        source_code: &str,
        function_node: &Node,
    ) -> Option<(usize, usize)> {
        let root = tree.root_node();
        let mut cursor = root.walk();

        // Find all top-level nodes (children of source_file)
        if !cursor.goto_first_child() {
            return None;
        }

        let mut preceding_attributes = Vec::new();
        let mut found_function = false;

        loop {
            let current_node = cursor.node();

            // If we found our target function, stop
            if current_node.id() == function_node.id() {
                found_function = true;
                break;
            }

            // Track attribute_items
            if current_node.kind() == "attribute_item" {
                preceding_attributes.push(current_node);
            } else if current_node.kind() != "attribute_item" {
                // Non-attribute node resets our attribute collection
                // (they don't belong to our function)
                preceding_attributes.clear();
            }

            if !cursor.goto_next_sibling() {
                break;
            }
        }

        if !found_function || preceding_attributes.is_empty() {
            return None;
        }

        // Verify the attributes are immediately before the function (only whitespace in between)
        let last_attr = preceding_attributes.last()?;
        let last_attr_end = last_attr.end_byte();
        let function_start = function_node.start_byte();

        let between_text = &source_code[last_attr_end..function_start];
        if !between_text.trim().is_empty() {
            // There's non-whitespace content between the attribute and function
            return None;
        }

        // Return range from first attribute to end of function
        let first_attr = preceding_attributes.first()?;
        Some((first_attr.start_byte(), function_node.end_byte()))
    }

    /// Validate replacement with custom byte range
    fn validate_replacement_with_range(
        original_code: &str,
        start_byte: usize,
        end_byte: usize,
        replacement: &str,
    ) -> Result<bool> {
        let rope = Rope::from_str(original_code);
        let start_char = rope.byte_to_char(start_byte);
        let end_char = rope.byte_to_char(end_byte);

        let mut temp_rope = rope.clone();
        temp_rope.remove(start_char..end_char);
        temp_rope.insert(start_char, replacement);

        let temp_code = temp_rope.to_string();
        crate::parsers::rust::RustParser::validate_rust_syntax(&temp_code)
    }

    fn insert_before_node(
        tree: &Tree,
        source_code: &str,
        selector: &NodeSelector,
        content: &str,
    ) -> Result<EditResult> {
        let node = selector
            .find_node_with_suggestions(tree, source_code, "rust")?
            .ok_or_else(|| anyhow!("Target node not found"))?;

        let rope = Rope::from_str(source_code);
        let start_byte = node.start_byte();
        let start_char = rope.byte_to_char(start_byte);

        // Find the appropriate indentation
        let line_start = rope.line_to_char(rope.char_to_line(start_char));
        let line_content = rope.slice(line_start..start_char).to_string();
        let indentation = line_content
            .chars()
            .take_while(|c| c.is_whitespace())
            .collect::<String>();

        let content_with_newline = format!("{content}\n{indentation}");

        let mut new_rope = rope.clone();
        new_rope.insert(start_char, &content_with_newline);

        Ok(EditResult::Success {
            message: format!("Successfully inserted content before {} node", node.kind()),
            new_content: new_rope.to_string(),
            affected_range: (start_char, start_char + content_with_newline.len()),
        })
    }

    fn insert_after_node(
        tree: &Tree,
        source_code: &str,
        selector: &NodeSelector,
        content: &str,
    ) -> Result<EditResult> {
        let node = selector
            .find_node_with_suggestions(tree, source_code, "rust")?
            .ok_or_else(|| anyhow!("Target node not found"))?;

        let rope = Rope::from_str(source_code);
        let end_byte = node.end_byte();
        let end_char = rope.byte_to_char(end_byte);

        // Find the appropriate indentation by looking at the node's line
        let start_char = rope.byte_to_char(node.start_byte());
        let line_start = rope.line_to_char(rope.char_to_line(start_char));
        let line_content = rope.slice(line_start..start_char).to_string();
        let indentation = line_content
            .chars()
            .take_while(|c| c.is_whitespace())
            .collect::<String>();

        let content_with_newline = format!("\n{indentation}{content}");

        let mut new_rope = rope.clone();
        new_rope.insert(end_char, &content_with_newline);

        Ok(EditResult::Success {
            message: format!("Successfully inserted content after {} node", node.kind()),
            new_content: new_rope.to_string(),
            affected_range: (end_char, end_char + content_with_newline.len()),
        })
    }

    fn wrap_node(
        tree: &Tree,
        source_code: &str,
        selector: &NodeSelector,
        wrapper_template: &str,
    ) -> Result<EditResult> {
        let node = selector
            .find_node_with_suggestions(tree, source_code, "rust")?
            .ok_or_else(|| anyhow!("Target node not found"))?;

        let node_text = get_node_text(&node, source_code);

        if !wrapper_template.contains("{{content}}") {
            return Err(anyhow!(
                "Wrapper template must contain {{content}} placeholder"
            ));
        }

        let wrapped_content = wrapper_template.replace("{{content}}", node_text);

        // Validate the wrapped content would create valid syntax
        if !Self::validate_replacement(source_code, &node, &wrapped_content)? {
            return Ok(EditResult::Error(
                "Wrapping would create invalid syntax".to_string(),
            ));
        }

        let rope = Rope::from_str(source_code);
        let start_byte = node.start_byte();
        let end_byte = node.end_byte();
        let start_char = rope.byte_to_char(start_byte);
        let end_char = rope.byte_to_char(end_byte);

        let mut new_rope = rope.clone();
        new_rope.remove(start_char..end_char);
        new_rope.insert(start_char, &wrapped_content);

        Ok(EditResult::Success {
            message: format!("Successfully wrapped {} node", node.kind()),
            new_content: new_rope.to_string(),
            affected_range: (start_char, start_char + wrapped_content.len()),
        })
    }

    fn delete_node(tree: &Tree, source_code: &str, selector: &NodeSelector) -> Result<EditResult> {
        let node = selector
            .find_node_with_suggestions(tree, source_code, "rust")?
            .ok_or_else(|| anyhow!("Target node not found"))?;

        let rope = Rope::from_str(source_code);
        let start_byte = node.start_byte();
        let end_byte = node.end_byte();
        let start_char = rope.byte_to_char(start_byte);
        let end_char = rope.byte_to_char(end_byte);

        let mut new_rope = rope.clone();
        new_rope.remove(start_char..end_char);

        Ok(EditResult::Success {
            message: format!("Successfully deleted {} node", node.kind()),
            new_content: new_rope.to_string(),
            affected_range: (start_char, start_char),
        })
    }

    fn validate_replacement(original_code: &str, node: &Node, replacement: &str) -> Result<bool> {
        // Create a temporary version with the replacement
        let rope = Rope::from_str(original_code);
        let start_char = rope.byte_to_char(node.start_byte());
        let end_char = rope.byte_to_char(node.end_byte());

        let mut temp_rope = rope.clone();
        temp_rope.remove(start_char..end_char);
        temp_rope.insert(start_char, replacement);

        let temp_code = temp_rope.to_string();

        // Parse and check for syntax errors
        crate::parsers::rust::RustParser::validate_rust_syntax(&temp_code)
    }

    pub fn format_code(source_code: &str) -> Result<String> {
        // For now, just return the original code
        // In a full implementation, we'd integrate with rustfmt
        Ok(source_code.to_string())
    }

    pub fn get_node_info(
        tree: &Tree,
        source_code: &str,
        selector: &NodeSelector,
    ) -> Result<String> {
        let node = selector
            .find_node_with_suggestions(tree, source_code, "rust")?
            .ok_or_else(|| anyhow!("Target node not found"))?;

        let node_text = get_node_text(&node, source_code);
        let start_pos = node.start_position();
        let end_pos = node.end_position();

        Ok(format!(
            "Node Information:\n\
            - Kind: {}\n\
            - Start: {}:{}\n\
            - End: {}:{}\n\
            - Byte range: {}-{}\n\
            - Content: {}\n",
            node.kind(),
            start_pos.row + 1,
            start_pos.column + 1,
            end_pos.row + 1,
            end_pos.column + 1,
            node.start_byte(),
            node.end_byte(),
            if node_text.len() > 100 {
                format!("{}...", &node_text[..100])
            } else {
                node_text.to_string()
            }
        ))
    }
}
