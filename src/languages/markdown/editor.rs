use crate::languages::traits::LanguageEditor;
use crate::operations::EditResult;
use crate::parser::get_node_text;
use anyhow::{anyhow, Result};
use ropey::Rope;
use tree_sitter::{Node, Tree};

pub struct MarkdownEditor;

impl Default for MarkdownEditor {
    fn default() -> Self {
        Self::new()
    }
}

impl MarkdownEditor {
    pub fn new() -> Self {
        Self
    }

    fn ensure_proper_markdown_spacing(content: &str, node: &Node, before: bool) -> String {
        match node.kind() {
            "atx_heading" | "section" => {
                // Headings and sections need blank lines around them
                if before {
                    if content.ends_with('\n') {
                        format!("{content}\n")
                    } else {
                        format!("{content}\n\n")
                    }
                } else if content.starts_with('\n') {
                    format!("\n{content}")
                } else {
                    format!("\n\n{content}")
                }
            }
            "paragraph" => {
                // Paragraphs need blank lines around them for separation
                if before {
                    if content.ends_with('\n') {
                        format!("{content}\n")
                    } else {
                        format!("{content}\n\n")
                    }
                } else if content.starts_with('\n') {
                    format!("\n{content}")
                } else {
                    format!("\n\n{content}")
                }
            }
            "fenced_code_block" => {
                // Code blocks need blank lines around them
                if before {
                    if content.ends_with('\n') {
                        format!("{content}\n")
                    } else {
                        format!("{content}\n\n")
                    }
                } else if content.starts_with('\n') {
                    format!("\n{content}")
                } else {
                    format!("\n\n{content}")
                }
            }
            "list_item" => {
                // List items just need single newlines
                if before {
                    if content.ends_with('\n') {
                        content.to_string()
                    } else {
                        format!("{content}\n")
                    }
                } else if content.starts_with('\n') {
                    content.to_string()
                } else {
                    format!("\n{content}")
                }
            }
            _ => content.to_string(),
        }
    }

    fn adjust_deletion_range_for_spacing(
        rope: &Rope,
        start_char: usize,
        end_char: usize,
        node: &Node,
    ) -> (usize, usize) {
        match node.kind() {
            "atx_heading" | "paragraph" | "fenced_code_block" | "section" => {
                // Try to remove trailing blank line if it exists
                let total_chars = rope.len_chars();
                if end_char < total_chars {
                    let next_chars: String = rope
                        .slice(end_char..std::cmp::min(end_char + 2, total_chars))
                        .into();
                    if next_chars.starts_with("\n\n") {
                        return (start_char, end_char + 2);
                    } else if next_chars.starts_with('\n') {
                        return (start_char, end_char + 1);
                    }
                }
                (start_char, end_char)
            }
            _ => (start_char, end_char),
        }
    }

    /// Try to apply smart deletion logic for common patterns where users
    /// replace content with empty string but likely mean to delete the container
    fn try_smart_deletion(node: &Node, source_code: &str) -> Result<Option<EditResult>> {
        // Check if this is heading content that should trigger smart deletion
        if let Some(heading_node) = Self::find_parent_heading(node) {
            return Ok(Some(Self::delete_heading_with_spacing(
                &heading_node,
                source_code,
            )?));
        }

        // Check if this is list item content that should trigger smart deletion
        if let Some(list_item_node) = Self::find_parent_list_item(node) {
            return Ok(Some(Self::delete_list_item_with_spacing(
                &list_item_node,
                source_code,
            )?));
        }

        // No smart deletion pattern detected
        Ok(None)
    }

    /// Find parent atx_heading node if the current node is content inside a heading
    fn find_parent_heading<'a>(node: &'a Node<'a>) -> Option<Node<'a>> {
        // Check if node is inline content inside an atx_heading
        if node.kind() == "inline" {
            if let Some(parent) = node.parent() {
                if parent.kind() == "atx_heading" {
                    return Some(parent);
                }
            }
        }
        None
    }

    /// Find parent list_item node if the current node is content inside a list item
    fn find_parent_list_item<'a>(node: &'a Node<'a>) -> Option<Node<'a>> {
        // Check if node is paragraph content inside a list_item
        if node.kind() == "paragraph" {
            if let Some(parent) = node.parent() {
                if parent.kind() == "list_item" {
                    return Some(parent);
                }
            }
        }
        // Also check if node is inline content inside paragraph inside list_item
        else if node.kind() == "inline" {
            if let Some(paragraph) = node.parent() {
                if paragraph.kind() == "paragraph" {
                    if let Some(list_item) = paragraph.parent() {
                        if list_item.kind() == "list_item" {
                            return Some(list_item);
                        }
                    }
                }
            }
        }
        None
    }

    /// Delete an entire heading with proper spacing cleanup
    fn delete_heading_with_spacing(heading_node: &Node, source_code: &str) -> Result<EditResult> {
        let rope = Rope::from_str(source_code);
        let start_byte = heading_node.start_byte();
        let end_byte = heading_node.end_byte();
        let start_char = rope.byte_to_char(start_byte);
        let end_char = rope.byte_to_char(end_byte);

        // Apply the same spacing adjustment logic as normal deletion
        let (final_start, final_end) =
            Self::adjust_deletion_range_for_spacing(&rope, start_char, end_char, heading_node);

        let mut new_rope = rope.clone();
        new_rope.remove(final_start..final_end);

        Ok(EditResult::Success {
            message: "Smart deletion: removed entire heading instead of creating empty heading"
                .to_string(),
            new_content: new_rope.to_string(),
        })
    }

    /// Delete an entire list item with proper spacing cleanup
    fn delete_list_item_with_spacing(
        list_item_node: &Node,
        source_code: &str,
    ) -> Result<EditResult> {
        let rope = Rope::from_str(source_code);
        let start_byte = list_item_node.start_byte();
        let end_byte = list_item_node.end_byte();
        let start_char = rope.byte_to_char(start_byte);
        let end_char = rope.byte_to_char(end_byte);

        // For list items, we typically want to include the trailing newline
        let final_end = if end_char < rope.len_chars() {
            let next_char: String = rope.slice(end_char..end_char + 1).into();
            if next_char == "\n" {
                end_char + 1
            } else {
                end_char
            }
        } else {
            end_char
        };

        let mut new_rope = rope.clone();
        new_rope.remove(start_char..final_end);

        Ok(EditResult::Success {
            message:
                "Smart deletion: removed entire list item instead of creating empty bullet point"
                    .to_string(),
            new_content: new_rope.to_string(),
        })
    }
}

impl LanguageEditor for MarkdownEditor {
    fn format_code(&self, source: &str) -> Result<String> {
        // For now, just return the original code
        // In a full implementation, we'd integrate with a Markdown formatter
        Ok(source.to_string())
    }

    fn replace<'tree>(
        &self,
        node: Node<'tree>,
        _tree: &Tree,
        source_code: &str,
        new_content: &str,
    ) -> Result<EditResult> {
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
        })
    }

    fn insert_before<'tree>(
        &self,
        node: Node<'tree>,
        _tree: &Tree,
        source_code: &str,
        content: &str,
    ) -> Result<EditResult> {
        let rope = Rope::from_str(source_code);
        let start_byte = node.start_byte();
        let start_char = rope.byte_to_char(start_byte);

        // For Markdown, we may need to add proper spacing
        let content_with_spacing = Self::ensure_proper_markdown_spacing(content, &node, true);

        let mut new_rope = rope.clone();
        new_rope.insert(start_char, &content_with_spacing);

        Ok(EditResult::Success {
            message: format!("Successfully inserted content before {} node", node.kind()),
            new_content: new_rope.to_string(),
        })
    }

    fn insert_after<'tree>(
        &self,
        node: Node<'tree>,
        _tree: &Tree,
        source_code: &str,
        content: &str,
    ) -> Result<EditResult> {
        let mut rope = Rope::from_str(source_code);
        let end_byte = node.end_byte();
        let end_char = rope.byte_to_char(end_byte);

        // For Markdown, we may need to add proper spacing
        let content_with_spacing = Self::ensure_proper_markdown_spacing(content, &node, false);

        rope.insert(end_char, &content_with_spacing);

        Ok(EditResult::Success {
            message: format!("Successfully inserted content after {} node", node.kind()),
            new_content: rope.to_string(),
        })
    }

    fn wrap<'tree>(
        &self,
        node: Node<'tree>,
        _tree: &Tree,
        source_code: &str,
        wrapper_template: &str,
    ) -> Result<EditResult> {
        let node_text = get_node_text(&node, source_code);

        if !wrapper_template.contains("{{content}}") {
            return Err(anyhow!(
                "Wrapper template must contain {{content}} placeholder"
            ));
        }

        let wrapped_content = wrapper_template.replace("{{content}}", node_text);

        let mut rope = Rope::from_str(source_code);
        let start_byte = node.start_byte();
        let end_byte = node.end_byte();
        let start_char = rope.byte_to_char(start_byte);
        let end_char = rope.byte_to_char(end_byte);

        rope.remove(start_char..end_char);
        rope.insert(start_char, &wrapped_content);

        Ok(EditResult::Success {
            message: format!("Successfully wrapped {} node", node.kind()),
            new_content: rope.to_string(),
        })
    }

    fn delete<'tree>(
        &self,
        node: Node<'tree>,
        _tree: &Tree,
        source_code: &str,
    ) -> Result<EditResult> {
        let mut rope = Rope::from_str(source_code);
        let start_byte = node.start_byte();
        let end_byte = node.end_byte();
        let start_char = rope.byte_to_char(start_byte);
        let end_char = rope.byte_to_char(end_byte);

        // Handle proper spacing for Markdown structure
        let (final_start, final_end) =
            Self::adjust_deletion_range_for_spacing(&rope, start_char, end_char, &node);

        rope.remove(final_start..final_end);

        Ok(EditResult::Success {
            message: format!("Successfully deleted {} node", node.kind()),
            new_content: rope.to_string(),
        })
    }
}
