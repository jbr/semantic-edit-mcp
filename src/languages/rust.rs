use std::{
    io::{Read, Write},
    process::{Command, Stdio},
};

use super::{
    semantic_grouping::{GroupingRule, SemanticGrouping, WithSemanticGrouping},
    traits::LanguageEditor,
    utils::parse_node_types_json,
    LanguageCommon,
};
use crate::{languages::LanguageName, operations::EditResult};
use anyhow::{anyhow, Result};
use ropey::Rope;
use tree_sitter::{Node, Tree};

pub fn language() -> Result<LanguageCommon> {
    let language = tree_sitter_rust::LANGUAGE.into();
    let validation_query = Some(tree_sitter::Query::new(
        &language,
        include_str!("../../queries/rust/validation.scm"),
    )?);
    let node_types = parse_node_types_json(tree_sitter_rust::NODE_TYPES)?;
    let editor = Box::new(RustEditor::new());

    Ok(LanguageCommon {
        language,
        validation_query,
        node_types,
        editor,
        name: LanguageName::Rust,
        file_extensions: &["rs"],
    })
}

#[derive(Default)]
pub struct RustLanguage;

impl SemanticGrouping for RustLanguage {
    fn get_grouping_rules(&self) -> Vec<GroupingRule> {
        vec![
            // Functions can have attributes and comments preceding them
            GroupingRule::new("function_item")
                .with_preceding_types(vec!["attribute_item", "line_comment", "block_comment"])
                .with_max_gap_nodes(2), // Allow some whitespace between elements
            // Structs can have attributes and comments
            GroupingRule::new("struct_item")
                .with_preceding_types(vec!["attribute_item", "line_comment", "block_comment"])
                .with_max_gap_nodes(2),
            // Enums can have attributes and comments
            GroupingRule::new("enum_item")
                .with_preceding_types(vec!["attribute_item", "line_comment", "block_comment"])
                .with_max_gap_nodes(2),
            // Impl blocks can have attributes and comments
            GroupingRule::new("impl_item")
                .with_preceding_types(vec!["attribute_item", "line_comment", "block_comment"])
                .with_max_gap_nodes(2),
            // Modules can have attributes and comments
            GroupingRule::new("mod_item")
                .with_preceding_types(vec!["attribute_item", "line_comment", "block_comment"])
                .with_max_gap_nodes(2),
        ]
    }
}

// WithSemanticGrouping is automatically implemented via blanket impl in semantic_grouping.rs

impl WithSemanticGrouping for RustLanguage {
    fn replacement_has_preceding_elements(&self, content: &str) -> bool {
        let trimmed = content.trim_start();
        // Check for Rust attributes or comments at the start
        trimmed.starts_with('#') || trimmed.starts_with("//") || trimmed.starts_with("/*")
    }
}

pub struct RustEditor {
    rust_language: RustLanguage,
}

impl Default for RustEditor {
    fn default() -> Self {
        Self::new()
    }
}

impl RustEditor {
    pub fn new() -> Self {
        Self {
            rust_language: RustLanguage,
        }
    }
}

impl LanguageEditor for RustEditor {
    fn format_code(&self, source: &str) -> Result<String> {
        let mut child = Command::new("rustfmt")
            .args(["--emit", "stdout", "--edition", "2024"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(source.as_bytes())?;
            drop(stdin);
        }

        let mut stdout = String::new();
        if let Some(mut out) = child.stdout.take() {
            out.read_to_string(&mut stdout)?;
        }

        let mut stderr = String::new();
        if let Some(mut err) = child.stderr.take() {
            err.read_to_string(&mut stderr)?;
        }

        if child.wait()?.success() {
            Ok(stdout)
        } else {
            Err(anyhow!(stderr))
        }
    }

    /// Replace a node using semantic grouping to determine the appropriate range
    fn replace<'tree>(
        &self,
        node: Node<'tree>,
        tree: &Tree,
        source_code: &str,
        new_content: &str,
    ) -> Result<EditResult> {
        // Use semantic grouping to calculate the replacement range
        let (actual_start_byte, actual_end_byte) =
            self.rust_language
                .calculate_replacement_range(tree, node, new_content, source_code)?;

        let mut rope = Rope::from_str(source_code);

        // Convert byte positions to character positions
        let start_char = rope.byte_to_char(actual_start_byte);
        let end_char = rope.byte_to_char(actual_end_byte);

        rope.remove(start_char..end_char);
        rope.insert(start_char, new_content);

        let message = format!("Successfully replaced {} node", node.kind());

        Ok(EditResult {
            message,
            new_content: rope.to_string(),
        })
    }

    /// Insert before a node using semantic grouping
    fn insert_before<'tree>(
        &self,
        node: Node<'tree>,
        tree: &Tree,
        source_code: &str,
        content: &str,
    ) -> Result<EditResult> {
        let (insert_pos, _) = self
            .rust_language
            .calculate_insertion_range(tree, node, true)?;

        let mut rope = Rope::from_str(source_code);
        let insert_char = rope.byte_to_char(insert_pos);
        rope.insert(insert_char, content);

        let message = format!("Successfully inserted content before {} node", node.kind());

        Ok(EditResult {
            message,
            new_content: rope.to_string(),
        })
    }

    /// Insert after a node using semantic grouping
    fn insert_after<'tree>(
        &self,
        node: Node<'tree>,
        tree: &Tree,
        source_code: &str,
        content: &str,
    ) -> Result<EditResult> {
        let (insert_pos, _) = self
            .rust_language
            .calculate_insertion_range(tree, node, false)?;

        let mut rope = Rope::from_str(source_code);
        let insert_char = rope.byte_to_char(insert_pos);
        rope.insert(insert_char, content);

        let message = format!("Successfully inserted content after {} node", node.kind());

        Ok(EditResult {
            message,
            new_content: rope.to_string(),
        })
    }

    /// Wrap a node using semantic grouping
    fn wrap<'tree>(
        &self,
        node: Node<'tree>,
        tree: &Tree,
        source_code: &str,
        wrapper_template: &str,
    ) -> Result<EditResult> {
        if !wrapper_template.contains("{{content}}") {
            return Err(anyhow!(
                "Wrapper template must contain {{content}} placeholder"
            ));
        }

        // Get the semantic group and its text
        let group = self.rust_language.find_semantic_group(tree, node)?;
        let (group_start, group_end) = group.byte_range();
        let group_text = &source_code[group_start..group_end];

        let wrapped_content = wrapper_template.replace("{{content}}", group_text);

        let mut rope = Rope::from_str(source_code);
        let start_char = rope.byte_to_char(group_start);
        let end_char = rope.byte_to_char(group_end);

        rope.remove(start_char..end_char);
        rope.insert(start_char, &wrapped_content);

        let message = if group.has_preceding_elements() || group.has_following_elements() {
            format!(
                "Successfully wrapped {} group ({} total elements)",
                node.kind(),
                group.all_nodes().len()
            )
        } else {
            format!("Successfully wrapped {} node", node.kind())
        };

        Ok(EditResult {
            message,
            new_content: rope.to_string(),
        })
    }

    /// Delete a node using semantic grouping
    fn delete(&self, node: Node, tree: &Tree, source_code: &str) -> Result<EditResult> {
        // Get the semantic group
        let group = self.rust_language.find_semantic_group(tree, node)?;
        let (group_start, group_end) = group.byte_range();

        let mut rope = Rope::from_str(source_code);
        let start_char = rope.byte_to_char(group_start);
        let end_char = rope.byte_to_char(group_end);
        rope.remove(start_char..end_char);

        let message = if group.has_preceding_elements() || group.has_following_elements() {
            format!(
                "Successfully deleted {} group ({} total elements)",
                node.kind(),
                group.all_nodes().len()
            )
        } else {
            format!("Successfully deleted {} node", node.kind())
        };

        Ok(EditResult {
            message,
            new_content: rope.to_string(),
        })
    }
}
