use anyhow::Result;
use tree_sitter::Tree;

use crate::languages::utils::collect_errors;

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
    fn format_code(&self, source: &str) -> Result<String> {
        Ok(source.to_string())
    }
}

impl LanguageEditor for DefaultEditor {
    // Uses all default implementations
}
