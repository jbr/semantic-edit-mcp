use anyhow::Result;

use tree_sitter::{Node, Tree};

use crate::editor::{Edit, EditIterator, Editor};

/// A language's candidate generation and syntax checks. Formatting is separate:
/// which formatter, if any, runs for a language is configured in
/// [`Formatting`](crate::formatting::Formatting).
pub trait LanguageEditor: Send + Sync {
    /// Collect syntax error line numbers from a tree-sitter parse tree
    fn collect_errors(&self, tree: &Tree, _content: &str) -> Vec<usize> {
        collect_errors(tree)
            .into_iter()
            .flat_map(|node| {
                let start = node.start_position().row;
                let end = node.end_position();
                let last = if end.column == 0 && end.row > start {
                    end.row - 1
                } else {
                    end.row
                };
                start..=last
            })
            .collect()
    }

    fn build_edits<'language, 'editor>(
        &self,
        editor: &'editor Editor<'language>,
    ) -> Result<Vec<Edit<'editor, 'language>>, String> {
        EditIterator::new(editor).find_edits()
    }
}

fn collect_errors<'tree>(tree: &'tree Tree) -> Vec<Node<'tree>> {
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
