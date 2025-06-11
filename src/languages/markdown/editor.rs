use crate::languages::traits::LanguageEditor;
use crate::operations::{EditOperation, EditResult, NodeSelector};
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

    fn apply_markdown_operation(
        operation: &EditOperation,
        source_code: &str,
    ) -> Result<EditResult> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_md::LANGUAGE.into())?;
        let tree = parser
            .parse(source_code, None)
            .ok_or_else(|| anyhow!("Failed to parse Markdown"))?;

        match operation {
            EditOperation::Replace {
                target,
                new_content,
                preview_only,
            } => {
                let mut result =
                    Self::replace_markdown_node(&tree, source_code, target, new_content)?;
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
                    Self::insert_before_markdown_node(&tree, source_code, target, content)?;
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
                let mut result =
                    Self::insert_after_markdown_node(&tree, source_code, target, content)?;
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
                    Self::wrap_markdown_node(&tree, source_code, target, wrapper_template)?;
                if preview_only.unwrap_or(false) {
                    result.set_message(format!("PREVIEW: {}", result.message()));
                }
                Ok(result)
            }
            EditOperation::Delete {
                target,
                preview_only,
            } => {
                let mut result = Self::delete_markdown_node(&tree, source_code, target)?;
                if preview_only.unwrap_or(false) {
                    result.set_message(format!("PREVIEW: {}", result.message()));
                }
                Ok(result)
            }
        }
    }

    fn replace_markdown_node(
        tree: &Tree,
        source_code: &str,
        selector: &NodeSelector,
        new_content: &str,
    ) -> Result<EditResult> {
        let node = selector
            .find_node(tree, source_code, "markdown")?
            .ok_or_else(|| anyhow!("Target node not found"))?;

        // Smart deletion: if user is replacing with empty content, check if they
        // really mean to delete the entire container (heading or list item)
        if new_content.trim().is_empty() {
            if let Some(deletion_result) = Self::try_smart_deletion(&node, source_code)? {
                return Ok(deletion_result);
            }
        }

        // Validate the new content would create valid Markdown
        if !Self::validate_markdown_replacement(source_code, &node, new_content)? {
            return Ok(EditResult::Error(
                "Replacement would create invalid Markdown structure".to_string(),
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
            affected_range: (final_start, final_start),
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
            affected_range: (start_char, start_char),
        })
    }

    fn insert_before_markdown_node(
        tree: &Tree,
        source_code: &str,
        selector: &NodeSelector,
        content: &str,
    ) -> Result<EditResult> {
        let node = selector
            .find_node(tree, source_code, "markdown")?
            .ok_or_else(|| anyhow!("Target node not found"))?;

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
            affected_range: (start_char, start_char + content_with_spacing.len()),
        })
    }

    fn insert_after_markdown_node(
        tree: &Tree,
        source_code: &str,
        selector: &NodeSelector,
        content: &str,
    ) -> Result<EditResult> {
        let node = selector
            .find_node(tree, source_code, "markdown")?
            .ok_or_else(|| anyhow!("Target node not found"))?;

        let rope = Rope::from_str(source_code);
        let end_byte = node.end_byte();
        let end_char = rope.byte_to_char(end_byte);

        // For Markdown, we may need to add proper spacing
        let content_with_spacing = Self::ensure_proper_markdown_spacing(content, &node, false);

        let mut new_rope = rope.clone();
        new_rope.insert(end_char, &content_with_spacing);

        Ok(EditResult::Success {
            message: format!("Successfully inserted content after {} node", node.kind()),
            new_content: new_rope.to_string(),
            affected_range: (end_char, end_char + content_with_spacing.len()),
        })
    }

    fn wrap_markdown_node(
        tree: &Tree,
        source_code: &str,
        selector: &NodeSelector,
        wrapper_template: &str,
    ) -> Result<EditResult> {
        let node = selector
            .find_node(tree, source_code, "markdown")?
            .ok_or_else(|| anyhow!("Target node not found"))?;

        let node_text = get_node_text(&node, source_code);

        if !wrapper_template.contains("{{content}}") {
            return Err(anyhow!(
                "Wrapper template must contain {{content}} placeholder"
            ));
        }

        let wrapped_content = wrapper_template.replace("{{content}}", node_text);

        // Validate the wrapped content would create valid Markdown
        if !Self::validate_markdown_replacement(source_code, &node, &wrapped_content)? {
            return Ok(EditResult::Error(
                "Wrapping would create invalid Markdown structure".to_string(),
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

    fn delete_markdown_node(
        tree: &Tree,
        source_code: &str,
        selector: &NodeSelector,
    ) -> Result<EditResult> {
        let node = selector
            .find_node(tree, source_code, "markdown")?
            .ok_or_else(|| anyhow!("Target node not found"))?;

        let rope = Rope::from_str(source_code);
        let start_byte = node.start_byte();
        let end_byte = node.end_byte();
        let start_char = rope.byte_to_char(start_byte);
        let end_char = rope.byte_to_char(end_byte);

        // Handle proper spacing for Markdown structure
        let (final_start, final_end) =
            Self::adjust_deletion_range_for_spacing(&rope, start_char, end_char, &node);

        let mut new_rope = rope.clone();
        new_rope.remove(final_start..final_end);

        Ok(EditResult::Success {
            message: format!("Successfully deleted {} node", node.kind()),
            new_content: new_rope.to_string(),
            affected_range: (final_start, final_start),
        })
    }

    fn validate_markdown_replacement(
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

        // Parse and check for Markdown syntax errors
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_md::LANGUAGE.into())?;

        if let Some(tree) = parser.parse(&temp_code, None) {
            Ok(!tree.root_node().has_error())
        } else {
            Ok(false)
        }
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
}

impl LanguageEditor for MarkdownEditor {
    fn apply_operation(&self, operation: &EditOperation, source: &str) -> Result<EditResult> {
        Self::apply_markdown_operation(operation, source)
    }

    fn get_node_info(&self, tree: &Tree, source: &str, selector: &NodeSelector) -> Result<String> {
        let node = selector
            .find_node(tree, source, "markdown")?
            .ok_or_else(|| anyhow!("Target node not found"))?;

        let node_text = get_node_text(&node, source);
        let start_pos = node.start_position();
        let end_pos = node.end_position();

        // Provide more detailed info for different Markdown node types
        let extra_info = match node.kind() {
            "atx_heading" => {
                // Try to get heading level and content
                let level = if let Some(marker) = node.child(0) {
                    match marker.kind() {
                        "atx_h1_marker" => "1",
                        "atx_h2_marker" => "2",
                        "atx_h3_marker" => "3",
                        "atx_h4_marker" => "4",
                        "atx_h5_marker" => "5",
                        "atx_h6_marker" => "6",
                        _ => "unknown",
                    }
                } else {
                    "unknown"
                };
                format!("\n- Heading level: {level}")
            }
            "fenced_code_block" => {
                // Try to get the language
                let language = if let Some(info_string) = node.child_by_field_name("info_string") {
                    get_node_text(&info_string, source).trim()
                } else {
                    "none"
                };
                format!("\n- Code language: {language}")
            }
            "link" => {
                // Try to get link text and destination
                let mut link_info = String::new();
                if let Some(link_text) = node.child_by_field_name("link_text") {
                    link_info.push_str(&format!(
                        "\n- Link text: {}",
                        get_node_text(&link_text, source).trim()
                    ));
                }
                if let Some(link_dest) = node.child_by_field_name("link_destination") {
                    link_info.push_str(&format!(
                        "\n- Link URL: {}",
                        get_node_text(&link_dest, source).trim()
                    ));
                }
                link_info
            }
            _ => String::new(),
        };

        Ok(format!(
            "Markdown Node Information:\n\
            - Kind: {}\n\
            - Start: {}:{}\n\
            - End: {}:{}\n\
            - Byte range: {}-{}{}\n\
            - Content: {}\n",
            node.kind(),
            start_pos.row + 1,
            start_pos.column + 1,
            end_pos.row + 1,
            end_pos.column + 1,
            node.start_byte(),
            node.end_byte(),
            extra_info,
            if node_text.len() > 100 {
                format!("{}...", &node_text[..100])
            } else {
                node_text.to_string()
            }
        ))
    }

    fn format_code(&self, source: &str) -> Result<String> {
        // For now, just return the original code
        // In a full implementation, we'd integrate with a Markdown formatter
        Ok(source.to_string())
    }

    fn validate_replacement(&self, original: &str, node: &Node, replacement: &str) -> Result<bool> {
        Self::validate_markdown_replacement(original, node, replacement)
    }
}
