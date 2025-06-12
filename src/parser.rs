use tree_sitter::{Node, Tree};

use crate::languages::LanguageRegistry;

pub fn detect_language_from_path(file_path: &str) -> Option<&'static str> {
    LanguageRegistry::new()
        .ok()?
        .detect_language_from_path(file_path)
}

pub fn get_node_text<'a>(node: &Node, source_code: &'a str) -> &'a str {
    &source_code[node.start_byte()..node.end_byte()]
}

pub fn find_node_by_position<'a>(tree: &'a Tree, line: usize, column: usize) -> Option<Node<'a>> {
    let point = tree_sitter::Point::new(line.saturating_sub(1), column.saturating_sub(1));
    let mut node = tree.root_node().descendant_for_point_range(point, point)?;

    // Walk up the tree to find a more "meaningful" node for editing
    // Skip trivial nodes like punctuation, identifiers, and literals
    while is_trivial_node(&node) && node.parent().is_some() {
        if let Some(parent) = node.parent() {
            node = parent;
        } else {
            break;
        }
    }

    Some(node)
}

/// Determines if a node is "trivial" and should be skipped for semantic selection
fn is_trivial_node(node: &Node) -> bool {
    match node.kind() {
        // Skip punctuation and delimiters
        "(" | ")" | "{" | "}" | "[" | "]" | ";" | "," | "." | ":" | "::" | "!" => true,
        // Skip small tokens that are rarely useful for editing
        "identifier" | "string_content" | "integer_literal" | "float_literal" => true,
        // Skip keywords unless they're at the start of a meaningful construct
        "fn" | "struct" | "impl" | "let" | "mut" | "pub" => {
            // Only skip if parent exists and is more meaningful
            node.parent().is_some_and(|parent| {
                matches!(
                    parent.kind(),
                    "function_item" | "struct_item" | "impl_item" | "let_declaration"
                )
            })
        }
        _ => false,
    }
}
