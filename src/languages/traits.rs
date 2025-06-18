use anyhow::{anyhow, Result};
use ropey::Rope;
use tree_sitter::{Node, Tree};

use crate::{
    languages::utils::collect_errors,
    operations::{selector::Position, EditOperation, EditResult},
};

/// Information about a node type from tree-sitter's node-types.json
#[derive(Debug, Clone)]
pub struct NodeTypeInfo {
    pub node_type: String, // from node-types.json: "function_item", "object"
    pub named: bool,       // from node-types.json
}

impl NodeTypeInfo {
    pub fn new(node_type: String, named: bool) -> Self {
        Self { node_type, named }
    }
}

/// Trait for language-specific editing operations
pub trait LanguageEditor: Send + Sync {
    /// Apply a generic edit operation
    fn apply_operation<'tree>(
        &self,
        node: Node<'tree>,
        tree: &Tree,
        operation: &EditOperation,
        source_code: &str,
    ) -> Result<EditResult> {
        let EditOperation { target, content } = operation;
        match (&target.position, content) {
            (None, _) => todo!(),
            (Some(Position::Before), Some(content)) => {
                self.insert_before(node, tree, source_code, content)
            }
            (Some(Position::After), Some(content)) => {
                self.insert_after(node, tree, source_code, content)
            }
            (Some(Position::Around), Some(content)) => self.wrap(node, tree, source_code, content),
            (Some(Position::Replace), Some(content)) => {
                self.replace(node, tree, source_code, content)
            }
            (Some(Position::Replace), None) => self.delete(node, tree, source_code),
            (Some(op), None) => Err(anyhow!("Content required for {op:?}")),
        }
    }

    fn collect_errors(&self, tree: &Tree, content: &str) -> Vec<usize> {
        let _ = content;
        collect_errors(tree)
            .into_iter()
            .map(|node| node.start_position().row)
            .collect()
    }

    fn replace<'tree>(
        &self,
        node: Node<'tree>,
        _tree: &Tree,
        source: &str,
        content: &str,
    ) -> Result<EditResult> {
        let mut rope = Rope::from_str(source);
        let start_byte = node.start_byte();
        let end_byte = node.end_byte();

        let start_char = rope.byte_to_char(start_byte);
        let end_char = rope.byte_to_char(end_byte);

        rope.remove(start_char..end_char);
        rope.insert(start_char, content);

        Ok(EditResult {
            message: format!("Successfully replaced {} node", node.kind()),
            new_content: rope.to_string(),
        })
    }

    fn insert_before<'tree>(
        &self,
        node: Node<'tree>,
        _tree: &Tree,
        source_code: &str,
        content: &str,
    ) -> Result<EditResult> {
        let mut rope = Rope::from_str(source_code);
        let start_byte = node.start_byte();
        let start_char = rope.byte_to_char(start_byte);

        rope.insert(start_char, content);

        Ok(EditResult {
            message: format!("Successfully inserted content before {} node", node.kind()),
            new_content: rope.to_string(),
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

        rope.insert(end_char, content);

        Ok(EditResult {
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
        let node_text = &source_code[node.byte_range()];

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

        Ok(EditResult {
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

        rope.remove(start_char..end_char);

        Ok(EditResult {
            message: format!("Successfully deleted {} node", node.kind()),
            new_content: rope.to_string(),
        })
    }

    fn format_code(&self, source: &str) -> Result<String> {
        Ok(source.to_string())
    }
}
