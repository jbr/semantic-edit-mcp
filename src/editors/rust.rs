use crate::operations::{EditOperation, EditResult, NodeSelector};
use crate::parsers::{TreeSitterParser, get_node_text};
use anyhow::{Result, anyhow};
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
                    result.message = format!("PREVIEW: {}", result.message);
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
                    result.message = format!("PREVIEW: {}", result.message);
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
                    result.message = format!("PREVIEW: {}", result.message);
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
                    result.message = format!("PREVIEW: {}", result.message);
                }
                Ok(result)
            }
            EditOperation::Delete {
                target,
                preview_only,
            } => {
                let mut result = Self::delete_node(&tree, source_code, target)?;
                if preview_only.unwrap_or(false) {
                    result.message = format!("PREVIEW: {}", result.message);
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

        // Validate the new content would create valid syntax
        if !Self::validate_replacement(source_code, &node, new_content)? {
            return Ok(EditResult {
                success: false,
                message: "Replacement would create invalid syntax".to_string(),
                new_content: None,
                affected_range: None,
            });
        }

        let rope = Rope::from_str(source_code);
        let start_byte = node.start_byte();
        let end_byte = node.end_byte();

        // Convert byte positions to character positions
        let start_char = rope.byte_to_char(start_byte);
        let end_char = rope.byte_to_char(end_byte);

        // Create new rope with replacement
        let mut new_rope = rope.clone();
        new_rope.remove(start_char..end_char);
        new_rope.insert(start_char, new_content);

        Ok(EditResult {
            success: true,
            message: format!("Successfully replaced {} node", node.kind()),
            new_content: Some(new_rope.to_string()),
            affected_range: Some((start_char, start_char + new_content.len())),
        })
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

        Ok(EditResult {
            success: true,
            message: format!("Successfully inserted content before {} node", node.kind()),
            new_content: Some(new_rope.to_string()),
            affected_range: Some((start_char, start_char + content_with_newline.len())),
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

        Ok(EditResult {
            success: true,
            message: format!("Successfully inserted content after {} node", node.kind()),
            new_content: Some(new_rope.to_string()),
            affected_range: Some((end_char, end_char + content_with_newline.len())),
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
            return Ok(EditResult {
                success: false,
                message: "Wrapping would create invalid syntax".to_string(),
                new_content: None,
                affected_range: None,
            });
        }

        let rope = Rope::from_str(source_code);
        let start_byte = node.start_byte();
        let end_byte = node.end_byte();
        let start_char = rope.byte_to_char(start_byte);
        let end_char = rope.byte_to_char(end_byte);

        let mut new_rope = rope.clone();
        new_rope.remove(start_char..end_char);
        new_rope.insert(start_char, &wrapped_content);

        Ok(EditResult {
            success: true,
            message: format!("Successfully wrapped {} node", node.kind()),
            new_content: Some(new_rope.to_string()),
            affected_range: Some((start_char, start_char + wrapped_content.len())),
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

        Ok(EditResult {
            success: true,
            message: format!("Successfully deleted {} node", node.kind()),
            new_content: Some(new_rope.to_string()),
            affected_range: Some((start_char, start_char)),
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
