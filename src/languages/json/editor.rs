use crate::languages::traits::LanguageEditor;
use crate::operations::{EditOperation, EditResult, NodeSelector};
use crate::parsers::get_node_text;
use anyhow::{anyhow, Result};
use ropey::Rope;
use tree_sitter::{Node, Tree};

pub struct JsonEditor;

impl Default for JsonEditor {
    fn default() -> Self {
        Self::new()
    }
}

impl JsonEditor {
    pub fn new() -> Self {
        Self
    }

    fn apply_json_operation(operation: &EditOperation, source_code: &str) -> Result<EditResult> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_json::LANGUAGE.into())?;
        let tree = parser
            .parse(source_code, None)
            .ok_or_else(|| anyhow!("Failed to parse JSON"))?;

        match operation {
            EditOperation::Replace {
                target,
                new_content,
                preview_only,
            } => {
                let mut result = Self::replace_json_node(&tree, source_code, target, new_content)?;
                if preview_only.unwrap_or(false) {
                    result.set_message(format!("PREVIEW: {}", result.message()));
                }
                Ok(result)
            }
            EditOperation::InsertBefore {
                target,
                content,
                preview_only,
            } => {
                let mut result =
                    Self::insert_before_json_node(&tree, source_code, target, content)?;
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
                let mut result = Self::insert_after_json_node(&tree, source_code, target, content)?;
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
                let mut result =
                    Self::wrap_json_node(&tree, source_code, target, wrapper_template)?;
                if preview_only.unwrap_or(false) {
                    result.set_message(format!("PREVIEW: {}", result.message()));
                }
                Ok(result)
            }
            EditOperation::Delete {
                target,
                preview_only,
            } => {
                let mut result = Self::delete_json_node(&tree, source_code, target)?;
                if preview_only.unwrap_or(false) {
                    result.set_message(format!("PREVIEW: {}", result.message()));
                }
                Ok(result)
            }
        }
    }

    fn replace_json_node(
        tree: &Tree,
        source_code: &str,
        selector: &NodeSelector,
        new_content: &str,
    ) -> Result<EditResult> {
        let node = selector
            .find_node(tree, source_code, "json")?
            .ok_or_else(|| anyhow!("Target node not found"))?;

        // Validate the new content would create valid JSON
        if !Self::validate_json_replacement(source_code, &node, new_content)? {
            return Ok(EditResult::Error(
                "Replacement would create invalid JSON".to_string(),
            ));
        }

        let rope = Rope::from_str(source_code);
        let start_byte = node.start_byte();
        let end_byte = node.end_byte();

        let start_char = rope.byte_to_char(start_byte);
        let end_char = rope.byte_to_char(end_byte);

        let mut new_rope = rope.clone();
        new_rope.remove(start_char..end_char);
        new_rope.insert(start_char, new_content);

        Ok(EditResult::Success {
            message: format!("Successfully replaced {} node", node.kind()),
            new_content: new_rope.to_string(),
            affected_range: (start_char, start_char + new_content.len()),
        })
    }

    fn insert_before_json_node(
        tree: &Tree,
        source_code: &str,
        selector: &NodeSelector,
        content: &str,
    ) -> Result<EditResult> {
        let node = selector
            .find_node(tree, source_code, "json")?
            .ok_or_else(|| anyhow!("Target node not found"))?;

        let rope = Rope::from_str(source_code);
        let start_byte = node.start_byte();
        let start_char = rope.byte_to_char(start_byte);

        // For JSON, we need to handle commas properly
        let content_with_comma = match node.kind() {
            "pair" => {
                // If inserting before a property, add comma after new content
                if Self::needs_comma_after(&node) {
                    format!("{content},")
                } else {
                    content.to_string()
                }
            }
            _ => content.to_string(),
        };

        let mut new_rope = rope.clone();
        new_rope.insert(start_char, &content_with_comma);

        Ok(EditResult::Success {
            message: format!("Successfully inserted content before {} node", node.kind()),
            new_content: new_rope.to_string(),
            affected_range: (start_char, start_char + content_with_comma.len()),
        })
    }

    fn insert_after_json_node(
        tree: &Tree,
        source_code: &str,
        selector: &NodeSelector,
        content: &str,
    ) -> Result<EditResult> {
        let node = selector
            .find_node(tree, source_code, "json")?
            .ok_or_else(|| anyhow!("Target node not found"))?;

        let rope = Rope::from_str(source_code);
        let end_byte = node.end_byte();
        let end_char = rope.byte_to_char(end_byte);

        // For JSON, we need to handle commas properly
        let content_with_comma = match node.kind() {
            "pair" => {
                // If inserting after a property, add comma before new content
                if Self::needs_comma_before(&node) {
                    format!(",{content}")
                } else {
                    content.to_string()
                }
            }
            _ => content.to_string(),
        };

        let mut new_rope = rope.clone();
        new_rope.insert(end_char, &content_with_comma);

        Ok(EditResult::Success {
            message: format!("Successfully inserted content after {} node", node.kind()),
            new_content: new_rope.to_string(),
            affected_range: (end_char, end_char + content_with_comma.len()),
        })
    }

    fn wrap_json_node(
        tree: &Tree,
        source_code: &str,
        selector: &NodeSelector,
        wrapper_template: &str,
    ) -> Result<EditResult> {
        let node = selector
            .find_node(tree, source_code, "json")?
            .ok_or_else(|| anyhow!("Target node not found"))?;

        let node_text = get_node_text(&node, source_code);

        if !wrapper_template.contains("{{content}}") {
            return Err(anyhow!(
                "Wrapper template must contain {{content}} placeholder"
            ));
        }

        let wrapped_content = wrapper_template.replace("{{content}}", node_text);

        // Validate the wrapped content would create valid JSON
        if !Self::validate_json_replacement(source_code, &node, &wrapped_content)? {
            return Ok(EditResult::Error(
                "Wrapping would create invalid JSON".to_string(),
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

    fn delete_json_node(
        tree: &Tree,
        source_code: &str,
        selector: &NodeSelector,
    ) -> Result<EditResult> {
        let node = selector
            .find_node(tree, source_code, "json")?
            .ok_or_else(|| anyhow!("Target node not found"))?;

        let rope = Rope::from_str(source_code);
        let start_byte = node.start_byte();
        let end_byte = node.end_byte();
        let start_char = rope.byte_to_char(start_byte);
        let end_char = rope.byte_to_char(end_byte);

        // Handle comma removal for JSON objects/arrays
        let (final_start, final_end) = if node.kind() == "pair" {
            Self::adjust_deletion_range_for_comma(&rope, start_char, end_char, &node)
        } else {
            (start_char, end_char)
        };

        let mut new_rope = rope.clone();
        new_rope.remove(final_start..final_end);

        Ok(EditResult::Success {
            message: format!("Successfully deleted {} node", node.kind()),
            new_content: new_rope.to_string(),
            affected_range: (final_start, final_start),
        })
    }

    fn validate_json_replacement(
        original_code: &str,
        node: &Node,
        replacement: &str,
    ) -> Result<bool> {
        let rope = Rope::from_str(original_code);
        let start_char = rope.byte_to_char(node.start_byte());
        let end_char = rope.byte_to_char(node.end_byte());

        let mut temp_rope = rope.clone();
        temp_rope.remove(start_char..end_char);
        temp_rope.insert(start_char, replacement);

        let temp_code = temp_rope.to_string();

        // Parse and check for JSON syntax errors
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_json::LANGUAGE.into())?;

        if let Some(tree) = parser.parse(&temp_code, None) {
            Ok(!tree.root_node().has_error())
        } else {
            Ok(false)
        }
    }

    fn needs_comma_after(node: &Node) -> bool {
        // Check if this node is followed by another sibling in an object or array
        if let Some(next_sibling) = node.next_sibling() {
            matches!(next_sibling.kind(), "pair" | "value")
        } else {
            false
        }
    }

    fn needs_comma_before(node: &Node) -> bool {
        // Check if this node is preceded by another sibling in an object or array
        if let Some(prev_sibling) = node.prev_sibling() {
            matches!(prev_sibling.kind(), "pair" | "value")
        } else {
            false
        }
    }

    fn adjust_deletion_range_for_comma(
        rope: &Rope,
        start_char: usize,
        end_char: usize,
        node: &Node,
    ) -> (usize, usize) {
        // If deleting a pair, also remove associated comma
        if let Some(next_sibling) = node.next_sibling() {
            if next_sibling.kind() == "," {
                let comma_end = rope.byte_to_char(next_sibling.end_byte());
                return (start_char, comma_end);
            }
        }

        if let Some(prev_sibling) = node.prev_sibling() {
            if prev_sibling.kind() == "," {
                let comma_start = rope.byte_to_char(prev_sibling.start_byte());
                return (comma_start, end_char);
            }
        }

        (start_char, end_char)
    }
}

impl LanguageEditor for JsonEditor {
    fn apply_operation(&self, operation: &EditOperation, source: &str) -> Result<EditResult> {
        Self::apply_json_operation(operation, source)
    }

    fn get_node_info(&self, tree: &Tree, source: &str, selector: &NodeSelector) -> Result<String> {
        let node = selector
            .find_node(tree, source, "json")?
            .ok_or_else(|| anyhow!("Target node not found"))?;

        let node_text = get_node_text(&node, source);
        let start_pos = node.start_position();
        let end_pos = node.end_position();

        Ok(format!(
            "JSON Node Information:\n\
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

    fn format_code(&self, source: &str) -> Result<String> {
        // For now, just return the original code
        // In a full implementation, we'd integrate with a JSON formatter
        Ok(source.to_string())
    }

    fn validate_replacement(&self, original: &str, node: &Node, replacement: &str) -> Result<bool> {
        Self::validate_json_replacement(original, node, replacement)
    }
}
