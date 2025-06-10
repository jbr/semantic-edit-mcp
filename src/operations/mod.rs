//! Operations module for semantic editing
//!
//! This module provides the core text-anchored node selection and editing operations.
//! The design uses content as anchor points and AST structure for precise targeting.

pub mod edit_operation;
pub mod selector;
pub mod validation;

// Re-export main types for convenience
pub use edit_operation::{EditOperation, EditResult};
pub use selector::NodeSelector;

use anyhow::Result;
use tree_sitter::Node;

/// Find an ancestor node of a specified type
pub fn find_ancestor_of_type<'a>(node: &Node<'a>, target_type: &str) -> Option<Node<'a>> {
    let mut current = *node;
    while let Some(parent) = current.parent() {
        if parent.kind() == target_type {
            return Some(parent);
        }
        current = parent;
    }
    None
}

/// Legacy support - will be removed in future versions
#[deprecated(note = "Use the new text-anchored NodeSelector instead")]
pub fn check_terrible_target(
    selector: &NodeSelector,
    target_node: &tree_sitter::Node<'_>,
    tree: &tree_sitter::Tree,
    source_code: &str,
    language: &str,
) -> Result<Option<String>> {
    validation::check_terrible_target(selector, target_node, tree, source_code, language)
}
