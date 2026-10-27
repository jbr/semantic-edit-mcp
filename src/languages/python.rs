use crate::{
    editor::{Edit, EditIterator, Editor},
    languages::{LanguageCommon, LanguageName, traits::LanguageEditor},
    layout::{Content, Reading, Target},
};
use anyhow::Result;
use tree_sitter::{Node, Parser, Query, Tree};

/// The node kinds that are string literals. Layout never re-indents a line that
/// begins inside one (f-strings included, interpolations and all).
const STRING_LITERALS: &[&str] = &["string"];

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

        let mut parser = editor
            .language()
            .tree_sitter_parser()
            .map_err(|e| e.to_string())?;
        let mut laid_out: Vec<Edit<'editor, 'language>> = Vec::with_capacity(edits.len() * 2);
        for edit in edits {
            for candidate in lay_out(edit, editor.tree(), &mut parser) {
                // Candidates found through different nodes often land identically;
                // trying one again would only rerun validation and the formatter.
                if !laid_out.iter().any(|seen| {
                    seen.position() == candidate.position() && seen.content() == candidate.content()
                }) {
                    laid_out.push(candidate);
                }
            }
        }

        Ok(laid_out)
    }
}

/// Lay out `edit`'s content at its target (see [`crate::layout`]), one candidate per
/// reading, the evidence's first. Indentation is syntax in Python, so when content
/// could be read either way both readings are candidates and validation chooses:
/// `if x:\n    y = 1` replacing a statement at column 4 is only valid read dedented.
fn lay_out<'editor, 'language>(
    edit: Edit<'editor, 'language>,
    tree: &Tree,
    parser: &mut Parser,
) -> Vec<Edit<'editor, 'language>> {
    let source = edit.source_code();
    let start = edit.start_byte();
    let end = edit.position().end_byte;
    // An insert after a whole statement lands at the end of the statement's line,
    // where the naive splice glues the new content onto that line. When the glued
    // result happens to be valid Python (`a = b` + `c = d` becoming the chained
    // `a = bc = d`) it is wrongly accepted, so the content goes on lines of its own,
    // at the statement's column.
    let target = match statement_ending_at(&edit, source, start) {
        Some(statement) => Target::after_line(source, start, statement.start_byte()),
        None => Target::at(source, start, end.is_some()),
    };

    let mut content = Content::new(edit.content());
    mark_string_lines(&mut content, &target, source, tree, end, parser);
    content
        .readings(&target)
        .into_iter()
        .map(|reading| {
            let placement = content.render(&target, reading);
            edit.clone()
                .with_start_byte(placement.start())
                .with_content(placement.into_text())
        })
        .collect()
}

/// Mark which of `content`'s lines would begin inside a string literal, from a parse
/// of the edit's result, so a line is recognized whether its literal opens in the
/// content or in the file around it (an edit inside a multi-line string).
///
/// Re-indenting lines outside literals can't move a literal's quotes, so any
/// reading's parse finds the same literals — except that tree-sitter's Python
/// scanner doesn't scan string contents while it recovers from an error. If the
/// file-columns parse has one, the dedented reading is parsed too, and unless that
/// parse is clean, a line either one places in a literal is kept as written. If
/// nothing parses, no line is moved.
fn mark_string_lines(
    content: &mut Content<'_>,
    target: &Target<'_>,
    source: &str,
    tree: &Tree,
    end: Option<usize>,
    parser: &mut Parser,
) {
    let mut parse = |content: &Content<'_>, reading| {
        let placement = content.render(target, reading);
        let tree = placement.reparse(parser, tree, source, end)?;
        Some((
            placement.lines_inside(&tree, STRING_LITERALS),
            tree.root_node().has_error(),
        ))
    };

    let unknown = vec![true; content.line_count()];
    let verbatim = match parse(content, Reading::FileColumns) {
        None => unknown,
        Some((verbatim, false)) => verbatim,
        Some((verbatim, true)) => {
            content.set_verbatim(verbatim.clone());
            match parse(content, Reading::Dedented) {
                None => unknown,
                Some((dedented, false)) => dedented,
                Some((dedented, true)) => verbatim
                    .into_iter()
                    .zip(dedented)
                    .map(|(either, or)| either || or)
                    .collect(),
            }
        }
    };
    content.set_verbatim(verbatim);
}

/// The statement an insert at `start` follows, when `start` is the end of a complete
/// statement: an insert (no `end_byte`) positioned exactly at the end of a node whose
/// parent is a statement container (`block`/`module`), with only whitespace
/// remaining on the line. This is the case that would otherwise glue a new statement
/// onto the anchor's line.
///
/// Keying on the node's *role* (statement vs. sub-expression) and on being at the
/// node's *end* is what keeps this from firing on `insert_before` (which sits at a
/// node's start) or on same-line appends like `insert_after 'def f(self'` → `,
/// arg` (whose anchor resolves inside the parameter list, not a statement
/// container).
fn statement_ending_at<'editor>(
    edit: &Edit<'editor, '_>,
    source: &str,
    start: usize,
) -> Option<Node<'editor>> {
    if edit.position().end_byte.is_some() {
        return None; // a replace, not an insert
    }

    // Only when the rest of the anchor's line is blank — i.e. we really are at the
    // line's end, not before a `;`-separated follow-on statement.
    let line_tail = source[start..].split('\n').next().unwrap_or("");
    if !line_tail.trim().is_empty() {
        return None;
    }

    // The captured node is often a sub-expression (e.g. the `identifier` in an
    // assignment), so climb the ancestors that also end exactly at the insertion
    // point. If one of them is a direct child of a statement container, the
    // insertion point is a statement boundary.
    let mut current = *edit.nodes()?.last()?;
    while current.end_byte() == start {
        let parent = current.parent()?;
        if matches!(parent.kind(), "block" | "module") {
            return Some(current);
        }
        current = parent;
    }
    None
}
