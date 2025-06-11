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
    // TODO: Re-implement terrible target detection without ast_explorer dependency
    // For now, return no terrible targets detected
    Ok(None)
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
