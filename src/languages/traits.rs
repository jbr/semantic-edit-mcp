use anyhow::{anyhow, Result};
use tree_sitter::{Node, Tree};

use crate::{
    languages::utils::collect_errors,
    operations::{EditOperation, EditResult},
};

/// Information about a node type from tree-sitter's node-types.json
#[derive(Debug, Clone)]
pub struct NodeTypeInfo {
    pub node_type: String,   // from node-types.json: "function_item", "object"
    pub named: bool,         // from node-types.json
    pub fields: Vec<String>, // from node-types.json: field names like "name", "body"
    pub supports_search_by_name: bool, // derived: has "name" field?
    pub display_name: String, // human-readable: "Function", "JSON Object"
}

impl NodeTypeInfo {
    pub fn new(node_type: String, named: bool, fields: Vec<String>) -> Self {
        let supports_search_by_name =
            fields.contains(&"name".to_string()) || fields.contains(&"key".to_string());

        let display_name = match node_type.as_str() {
            "function_item" => "Function".to_string(),
            "struct_item" => "Struct".to_string(),
            "impl_item" => "Implementation".to_string(),
            "enum_item" => "Enum".to_string(),
            "mod_item" => "Module".to_string(),
            "object" => "JSON Object".to_string(),
            "array" => "JSON Array".to_string(),
            "pair" => "JSON Property".to_string(),
            "table" => "TOML Table".to_string(),
            "atx_heading" => "Markdown Heading".to_string(),
            "fenced_code_block" => "Code Block".to_string(),
            _ => {
                // Convert snake_case to Title Case
                node_type
                    .split('_')
                    .map(|word| {
                        let mut chars = word.chars();
                        match chars.next() {
                            None => String::new(),
                            Some(first) => {
                                first.to_uppercase().collect::<String>() + chars.as_str()
                            }
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            }
        };

        Self {
            node_type,
            named,
            fields,
            supports_search_by_name,
            display_name,
        }
    }
}

/// Trait for language-specific parsing operations
pub trait LanguageParser: Send + Sync {
    /// Find a node by name (for nodes that have a "name" field)
    fn find_by_name<'a>(
        &self,
        tree: &'a Tree,
        source: &str,
        node_type: &str,
        name: &str,
    ) -> Result<Option<Node<'a>>>;

    /// Find nodes by type
    fn find_by_type<'a>(&self, tree: &'a Tree, node_type: &str) -> Vec<Node<'a>>;

    /// Execute a custom tree-sitter query
    fn execute_query<'a>(
        &self,
        query_text: &str,
        tree: &'a Tree,
        source: &str,
    ) -> Result<Vec<Node<'a>>>;

    /// Validate syntax for this language
    fn validate_syntax(&self, source: &str) -> Result<bool>;

    /// Get all names for a specific node type (e.g., all function names)
    fn get_all_names(&self, tree: &Tree, source: &str, node_type: &str) -> Vec<String>;
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
        match operation {
            EditOperation::Replace { content, .. } => self.replace(
                node,
                tree,
                source_code,
                content
                    .as_deref()
                    .ok_or_else(|| anyhow!("expected content"))?,
            ),

            EditOperation::InsertBefore { content, .. } => self.insert_before(
                node,
                tree,
                source_code,
                content
                    .as_deref()
                    .ok_or_else(|| anyhow!("expected content"))?,
            ),

            EditOperation::InsertAfter { content, .. } => self.insert_after(
                node,
                tree,
                source_code,
                content
                    .as_deref()
                    .ok_or_else(|| anyhow!("expected content"))?,
            ),

            EditOperation::Wrap {
                wrapper_template, ..
            } => self.wrap(
                node,
                tree,
                source_code,
                wrapper_template
                    .as_deref()
                    .ok_or_else(|| anyhow!("expected wrapper template"))?,
            ),

            EditOperation::Delete { .. } => self.delete(node, tree, source_code),
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
        tree: &Tree,
        source: &str,
        content: &str,
    ) -> Result<EditResult>;

    fn insert_before<'tree>(
        &self,
        node: Node<'tree>,
        tree: &Tree,
        source: &str,
        content: &str,
    ) -> Result<EditResult>;

    fn insert_after<'tree>(
        &self,
        node: Node<'tree>,
        tree: &Tree,
        source: &str,
        content: &str,
    ) -> Result<EditResult>;

    fn wrap<'tree>(
        &self,
        node: Node<'tree>,
        tree: &Tree,
        source: &str,
        wrapper_template: &str,
    ) -> Result<EditResult>;

    fn delete<'tree>(&self, node: Node<'tree>, tree: &Tree, source: &str) -> Result<EditResult>;

    fn format_code(&self, source: &str) -> Result<String>;
}
