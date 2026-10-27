mod indentation;

use crate::{
    editor::{Edit, EditIterator, Editor},
    languages::{LanguageCommon, LanguageName, traits::LanguageEditor},
    layout::{Readings, Rules},
    validation::Diagnostic,
};
use anyhow::Result;
use tree_sitter::{Query, Tree};

pub fn language() -> LanguageCommon {
    let language = tree_sitter_python::LANGUAGE.into();
    let query = Query::new(
        &language,
        include_str!("../../queries/python/validation.scm"),
    )
    .unwrap();

    LanguageCommon {
        name: LanguageName::Python,
        file_extensions: &["py", "pyi"],
        language,
        editor: Box::new(PythonEditor),
        validation_query: Some(query),
    }
}

struct PythonEditor;

impl LanguageEditor for PythonEditor {
    /// Indentation is syntax, so when content could be read either way both
    /// readings are candidates and validation chooses: `if x:\n    y = 1`
    /// replacing a statement at column 4 is only valid read dedented. A line that
    /// begins inside a string literal (f-strings included, interpolations and all)
    /// is never re-indented. And a compound statement ends at the same byte as the
    /// last statement in its block, so an insert after a node follows the
    /// innermost statement holding it, at that statement's column: after
    /// `return x`, not after the `def` whose block `return x` ends.
    fn layout(&self) -> Rules {
        Rules {
            literals: &["string"],
            readings: Readings::Both,
            statements: &["block", "module"],
            prose: false,
        }
    }

    fn build_edits<'language, 'editor>(
        &self,
        editor: &'editor Editor<'language>,
    ) -> Result<Vec<Edit<'editor, 'language>>, String> {
        let edit_iterator = EditIterator::new(editor);

        let mut edits = edit_iterator.find_edits()?;

        let additional_edits = edits
            .iter()
            .filter_map(|edit| {
                edit.nodes()
                    .and_then(|nodes| {
                        nodes.iter().find_map(|node| {
                            node.children(&mut node.walk())
                                .find(|node| node.kind() == "block")
                        })
                    })
                    .map(|block| {
                        [
                            edit.clone()
                                .with_nodes(vec![block])
                                .with_start_byte(block.start_byte())
                                .with_annotation("python: inside block"),
                            edit.clone()
                                .with_nodes(vec![block])
                                .with_start_byte(block.start_byte())
                                .with_content(format!("{}\n", edit.content()))
                                .with_annotation("python: inside block with newline"),
                        ]
                    })
            })
            .flatten()
            .collect::<Vec<_>>();
        edits.extend(additional_edits);

        Ok(edits)
    }

    /// The language's indentation rules, which tree-sitter's grammar doesn't
    /// enforce: a block header with nothing indented under it, a line indented
    /// where no block opens, a dedent to no enclosing level, and indentation
    /// whose meaning depends on the width of a tab. Every logged Python edit a
    /// formatter refused failed one of these. A full parser would lag the
    /// language; these rules haven't changed since Python 3.0.
    fn check(&self, tree: &Tree, source: &str) -> Result<(), Diagnostic> {
        indentation::check(tree, source)
    }
}
