use std::path::Path;

use anyhow::Result;

use tree_sitter::{Node, Tree};

use crate::editor::{Edit, EditIterator, Editor};

/// Default editor implementation with basic tree-sitter validation
#[derive(Debug, Clone)]
pub struct DefaultEditor;

impl DefaultEditor {
    pub fn new() -> Self {
        Self
    }
}

impl Default for DefaultEditor {
    fn default() -> Self {
        Self::new()
    }
}

/// Trait for language-specific operations like validation and formatting
pub trait LanguageEditor: Send + Sync {
    /// Collect syntax error line numbers from a tree-sitter parse tree
    fn collect_errors(&self, tree: &Tree, _content: &str) -> Vec<usize> {
        collect_errors(tree)
            .into_iter()
            .map(|node| node.start_position().row)
            .collect()
    }

    /// Format code according to language conventions
    fn format_code(&self, source: &str, file_path: &Path) -> Result<String> {
        let _ = file_path;
        Ok(source.to_string())
    }

    fn build_edits<'language, 'editor>(
        &self,
        editor: &'editor Editor<'language>,
    ) -> Result<Vec<Edit<'editor, 'language>>, String> {
        EditIterator::new(editor).find_edits()
    }
}

impl LanguageEditor for DefaultEditor {
    // Uses all default implementations
}

pub fn collect_errors<'tree>(tree: &'tree Tree) -> Vec<Node<'tree>> {
    let mut errors = vec![];
    collect_errors_recursive(tree.root_node(), &mut errors);
    errors
}

fn collect_errors_recursive<'tree>(node: Node<'tree>, errors: &mut Vec<Node<'tree>>) {
    // Check if this node is an error
    if node.is_error() || node.is_missing() {
        errors.push(node);
    }

    // Recursively check all children
    for child in node.children(&mut node.walk()) {
        collect_errors_recursive(child, errors);
    }
}
