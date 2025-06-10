use crate::operations::selector::NodeSelector;
use anyhow::Result;

/// Check for structural warnings and terrible targets
pub fn check_terrible_target(
    selector: &NodeSelector,
    target_node: &tree_sitter::Node<'_>,
    tree: &tree_sitter::Tree,
    source_code: &str,
    language: &str,
) -> Result<Option<String>> {
    use crate::ast_explorer::{ASTExplorer, EditSuitability};

    let node_info = ASTExplorer::analyze_node(target_node, source_code, language);

    if let EditSuitability::Terrible { reason, why_avoid } = node_info.edit_suitability {
        // For text-anchored selectors, we can't provide position-based exploration
        // since we don't have line/column info. Just return a simple error.
        return Ok(Some(format!(
            "❌ Edit blocked: {reason}\n🚫 {why_avoid}\n\n💡 Try using a different anchor_text or ancestor_node_type to find better targets.",
        )));
    }

    Ok(None) // No terrible target detected
}

/// Check for structural warnings (less severe than terrible targets)
pub fn check_structural_warning(
    operation: &crate::operations::edit_operation::EditOperation,
    target_node: &tree_sitter::Node<'_>,
) -> Result<Option<String>> {
    use crate::operations::edit_operation::EditOperation;

    let node_kind = target_node.kind();
    let parent_kind = target_node.parent().map(|p| p.kind());

    Ok(match operation {
        EditOperation::InsertAfter { .. } => {
            match node_kind {
                "impl_item" | "struct_item" | "enum_item" | "mod_item" => {
                    Some("You're inserting after a container block. Content will be placed OUTSIDE the container, not inside it.".to_string())
                }
                "function_item" if parent_kind == Some("impl_item") => {
                    Some("Inserting after this method will place content at module level, outside the impl block.".to_string())
                }
                "block" => {
                    Some("Inserting after a block will place content outside the block scope.".to_string())
                }
                _ => None
            }
        }
        EditOperation::Replace { .. } => {
            match node_kind {
                "impl_item" | "struct_item" | "enum_item" => {
                    Some("You're replacing an entire container definition. This will remove all its contents.".to_string())
                }
                _ => None
            }
        }
        _ => None
    })
}
