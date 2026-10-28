use anyhow::Result;

use tree_sitter::{Node, Tree};

use crate::{
    editor::{Edit, EditIterator, Editor},
    layout::Rules,
    validation::Diagnostic,
};

/// A language's candidate generation, layout and syntax checks. Formatting is
/// separate: which formatter, if any, runs for a language is configured in
/// [`Formatting`](crate::formatting::Formatting).
pub trait LanguageEditor: Send + Sync {
    /// How edit content is laid out in this language's files (see
    /// [`crate::layout`]): code whose indentation isn't syntax, with no
    /// multi-line literals, unless a language says otherwise.
    fn layout(&self) -> Rules {
        Rules::CODE
    }

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

    /// A check stricter than tree-sitter's, run in-process on a source that
    /// parses cleanly: `Err` with the checker's own diagnostic when the language
    /// rejects `source`, whose parse is `tree`. tree-sitter's grammars are built
    /// for editors, so they accept more than the language does; this is where a
    /// language says what they miss. A language without one accepts everything.
    ///
    /// A candidate edit fails the check only if the file passed it before the
    /// edit (see [`Editor`]), so a file the checker can't handle, because it uses
    /// syntax newer than the checker, stays editable.
    fn check(&self, _tree: &Tree, _source: &str) -> Result<(), Diagnostic> {
        Ok(())
    }
}

/// The ERROR and MISSING nodes in `tree`, in order. The walk goes only into
/// subtrees that hold one, and never recurses: a source nests as deep as it
/// likes, and overflowing the stack would end the process.
fn collect_errors<'tree>(tree: &'tree Tree) -> Vec<Node<'tree>> {
    let mut errors = vec![];
    let mut cursor = tree.walk();
    loop {
        let node = cursor.node();
        if node.is_error() || node.is_missing() {
            errors.push(node);
        }
        if node.has_error() && cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return errors;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::collect_errors;
    use tree_sitter::{Parser, Tree};

    fn parse(source: &str) -> Tree {
        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_rust::LANGUAGE.into())
            .unwrap();
        parser.parse(source, None).unwrap()
    }

    /// Nested this deep, a recursive walk overflowed the stack of the thread an
    /// edit runs on, which ended the process.
    #[test]
    fn a_deep_tree_is_walked_on_a_small_stack() {
        let depth = 50_000;
        let clean = format!("fn f() {{ let x = 1{}; }}\n", " + 1".repeat(depth));
        let broken = format!(
            "fn f() {{ let x = {}1 +{}; }}\n",
            "(".repeat(depth / 10),
            ")".repeat(depth / 10)
        );
        let (clean, broken) = std::thread::Builder::new()
            .stack_size(256 << 10)
            .spawn(move || {
                let clean = collect_errors(&parse(&clean)).len();
                let broken = collect_errors(&parse(&broken))
                    .iter()
                    .map(|node| node.start_position().row)
                    .collect::<Vec<_>>();
                (clean, broken)
            })
            .unwrap()
            .join()
            .unwrap();
        assert_eq!(clean, 0);
        assert_eq!(broken, vec![0]);
    }

    #[test]
    fn every_error_and_missing_node_is_found() {
        let tree = parse("fn f() {\n    let x = ;\n}\nfn g( {}\nstruct S { a: u8 }\n");
        let errors = collect_errors(&tree);
        assert!(errors.len() >= 2, "{}", tree.root_node().to_sexp());
        assert!(
            errors
                .iter()
                .all(|node| node.is_error() || node.is_missing())
        );
        let mut recursive = vec![];
        fn walk<'tree>(node: tree_sitter::Node<'tree>, found: &mut Vec<tree_sitter::Node<'tree>>) {
            if node.is_error() || node.is_missing() {
                found.push(node);
            }
            for child in node.children(&mut node.walk()) {
                walk(child, found);
            }
        }
        walk(tree.root_node(), &mut recursive);
        assert_eq!(errors, recursive);
    }
}
