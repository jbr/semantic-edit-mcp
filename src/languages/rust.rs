use crate::editor::{Edit, EditIterator, Editor};
use crate::selector::Operation;

use super::{LanguageCommon, LanguageName, traits::LanguageEditor};
use crate::{layout::Rules, validation::Diagnostic};
use anyhow::Result;
use std::thread;
use tree_sitter::{Node, Query, Tree};

pub fn language() -> LanguageCommon {
    let language = tree_sitter_rust::LANGUAGE.into();
    let query = Query::new(&language, include_str!("../../queries/rust/validation.scm")).unwrap();
    LanguageCommon {
        language,
        validation_query: Some(query),
        editor: Box::new(RustEditor),
        name: LanguageName::Rust,
        file_extensions: &["rs"],
    }
}

struct RustEditor;

impl LanguageEditor for RustEditor {
    /// A line inside a string or raw string literal is never re-indented. Block
    /// comments aren't literals here: their continuation lines (` * …`) sit at
    /// the code's column, so they move with it and count as evidence like code.
    fn layout(&self) -> Rules {
        Rules {
            literals: &["string_literal", "raw_string_literal"],
            ..Rules::CODE
        }
    }

    fn build_edits<'language, 'editor>(
        &self,
        editor: &'editor Editor<'language>,
    ) -> Result<Vec<Edit<'editor, 'language>>, String> {
        let edit_iterator = EditIterator::new(editor);
        let mut edits = edit_iterator.find_edits()?;
        if matches!(editor.selector().operation, Operation::Replace)
            && editor.content().trim().is_empty()
        {
            for edit in &mut edits {
                take_decorations(edit, editor.tree());
            }
            return Ok(edits);
        }
        let mut parser = editor
            .language()
            .tree_sitter_parser()
            .map_err(|e| e.to_string())?;
        let content_parse = parser.parse(editor.content(), None);

        if let Some(content_parse) = content_parse {
            for edit in &mut edits {
                let root = content_parse.root_node();
                let mut walk = root.walk();
                let replacement = root.children(&mut walk).collect::<Vec<_>>();

                // A doc comment's node takes its line's break, so replacing one
                // takes the break too, and content ending in a comment of its own
                // has to give it back. A `//` comment's node stops before its
                // break, which stays: adding another would leave a blank line.
                if let Some("line_comment") = replacement.last().map(|node| node.kind())
                    && edit
                        .edit_region()
                        .is_some_and(|region| region.ends_with('\n'))
                    && !editor.content().ends_with('\n')
                {
                    edit.content_mut().to_mut().push('\n');
                } else {
                    handle_grouping(edit, &replacement, editor.content());
                }
            }
        }

        Ok(edits)
    }

    /// syn's parse of the file, which rejects what rustc's parser does where
    /// tree-sitter's grammar doesn't: an item an `impl` can't hold, or a doc
    /// comment or attribute with no item after it.
    fn check(&self, tree: &Tree, source: &str) -> Result<(), Diagnostic> {
        if nests_deeper_than(tree, SYN_MAX_DEPTH) {
            return Ok(());
        }
        check_with_syn(source)
    }
}

/// How deep a parse may nest for syn to check it. syn's descent recurses as the
/// source nests, and overflowing a stack ends the process, which may be the
/// host's: on [`SYN_STACK_SIZE`], a debug build overflows at about 6,000 nested
/// parentheses. The deepest of 18,948 real files nests 73 deep.
const SYN_MAX_DEPTH: usize = 1_000;

/// The stack syn parses on: a reservation, which the thread touches only as
/// deep as the source nests.
const SYN_STACK_SIZE: usize = 64 << 20;

/// Whether `tree` nests deeper than `limit`, walked without recursion.
fn nests_deeper_than(tree: &Tree, limit: usize) -> bool {
    let mut cursor = tree.walk();
    let mut depth = 0;
    loop {
        if cursor.goto_first_child() {
            depth += 1;
            if depth > limit {
                return true;
            }
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return false;
            }
            depth -= 1;
        }
    }
}

/// `source` parsed by syn on a thread of its own.
///
/// For line and column, proc-macro2 keeps a copy of every source it lexes in a
/// thread-local map that only grows, and its spans index into that map. On a
/// thread of its own a check's copy is freed when the thread ends, and the map
/// never meets spans the host holds on its own threads. The thread also gives
/// syn a deep stack. If the thread can't start, or syn panics, the source isn't
/// rejected: tree-sitter has already accepted it.
fn check_with_syn(source: &str) -> Result<(), Diagnostic> {
    thread::scope(|scope| {
        thread::Builder::new()
            .name(String::from("syn"))
            .stack_size(SYN_STACK_SIZE)
            .spawn_scoped(scope, || syn_diagnostic(source))
            .ok()
            .and_then(|parse| parse.join().ok())
            .flatten()
            .map_or(Ok(()), Err)
    })
}

fn syn_diagnostic(source: &str) -> Option<Diagnostic> {
    let error = syn::parse_file(source).err()?;
    let span = error.span();
    // An error at the end of the input has the call site's span, which isn't in
    // the source: the input ran out, so the error is where the file ends.
    let (line, column) = if span.source_text().is_some() {
        let start = span.start();
        (start.line, Some(start.column + 1))
    } else {
        (source.trim_end().lines().count().max(1), None)
    };
    Some(Diagnostic {
        message: error.to_string(),
        line: Some(line),
        column,
    })
}

fn handle_grouping<'editor, 'language>(
    edit: &mut Edit<'editor, 'language>,
    replacement: &[Node<'_>],
    replacement_src: &str,
) -> Option<()> {
    let replacement_has_preceding = replacement.iter().any(|node| is_preceding_item(*node));
    let replacement_has_primary = replacement.iter().any(|node| !is_preceding_item(*node));

    // Keep expanding until no more expansions are needed
    while let Some(expansion_type) = determine_expansion_type(
        edit.nodes()?,
        replacement_has_preceding,
        replacement_has_primary,
    ) {
        let expanded_nodes: Vec<Node<'editor>> = match expansion_type {
            ExpansionType::BackwardsToMatchPattern => reconcile_backward(
                edit.nodes()?,
                replacement,
                edit.source_code(),
                replacement_src,
            )?,
            ExpansionType::ForwardToIncludePrimary => {
                expand_forward_to_include_primary(edit.nodes()?)?
            }
        };

        apply_expansion(edit, expanded_nodes, expansion_type);
    }

    Some(())
}

#[derive(Debug)]
enum ExpansionType {
    BackwardsToMatchPattern,
    ForwardToIncludePrimary,
}

/// A deletion of an item takes the outer attributes and doc comments that
/// decorate it. They apply to the item across the comments and blank lines
/// between, and only to it: left behind, they would decorate whatever follows,
/// or nothing, which doesn't parse. Only the start moves, so a separator the
/// anchor took still goes; plain comments above the topmost decoration stay,
/// and so do decorations a deletion of only decorations didn't take.
///
/// The item is the candidate's first node, or for a candidate that has none,
/// the outermost node that starts where it does and ends within it.
fn take_decorations<'editor>(edit: &mut Edit<'editor, '_>, tree: &'editor Tree) {
    let Some(end) = edit.position().end_byte else {
        return;
    };
    let item = match edit.nodes() {
        Some(nodes) if nodes.iter().all(|node| is_preceding_item(*node)) => return,
        Some(nodes) => nodes[0],
        None => match item_at(tree, edit.start_byte(), end) {
            Some(item) => item,
            None => return,
        },
    };
    let Some(decorations) = decorations_above(item) else {
        return;
    };
    edit.position_mut()
        .set_start_byte(decorations[0].start_byte());
    if let Some(nodes) = edit.take_nodes() {
        edit.set_nodes([decorations, nodes].concat());
    }
    edit.set_annotation("rust: deletion takes the item's decorations");
}

/// The outermost named node that starts at `start` and ends by `end`, when it
/// is an item rather than a comment or attribute.
fn item_at(tree: &Tree, start: usize, end: usize) -> Option<Node<'_>> {
    let mut node = tree
        .root_node()
        .named_descendant_for_byte_range(start, start + 1)?;
    if node.start_byte() != start {
        return None;
    }
    while let Some(parent) = node.parent().filter(|parent| {
        parent.parent().is_some() && parent.start_byte() == start && parent.end_byte() <= end
    }) {
        node = parent;
    }
    (!is_preceding_item(node)).then_some(node)
}

/// The decorations above `node`, top to bottom, from the topmost outer
/// attribute or doc comment through any comments between them and `node`, or
/// `None` when it has none.
fn decorations_above(node: Node<'_>) -> Option<Vec<Node<'_>>> {
    let mut above = Vec::new();
    let mut current = node;
    while let Some(previous) = current.prev_sibling() {
        if !is_outer_decoration(previous) && !is_plain_comment(previous) {
            break;
        }
        above.push(previous);
        current = previous;
    }
    let topmost = above.iter().rposition(|node| is_outer_decoration(*node))?;
    above.truncate(topmost + 1);
    above.reverse();
    Some(above)
}

/// An outer attribute or doc comment: `#[…]`, `///`, `/** */`.
fn is_outer_decoration(node: Node<'_>) -> bool {
    node.kind() == "attribute_item"
        || is_comment(node) && node.child_by_field_name("outer").is_some()
}

/// A comment that documents nothing: not `///`, `//!`, `/** */` or `/*! */`.
fn is_plain_comment(node: Node<'_>) -> bool {
    is_comment(node)
        && node.child_by_field_name("outer").is_none()
        && node.child_by_field_name("inner").is_none()
}

fn is_comment(node: Node<'_>) -> bool {
    matches!(node.kind(), "line_comment" | "block_comment")
}

fn determine_expansion_type<'editor>(
    edit_nodes: &[Node<'editor>],
    replacement_has_preceding: bool,
    replacement_has_primary: bool,
) -> Option<ExpansionType> {
    let selection_has_preceding = edit_nodes.iter().any(|node| is_preceding_item(*node));
    let selection_has_primary = edit_nodes.iter().any(|node| !is_preceding_item(*node));
    let selection_only_preceding = selection_has_preceding && !selection_has_primary;

    if replacement_has_preceding && replacement_has_primary && !selection_has_preceding {
        // Risk of duplication: expand backwards to include matching preceding items
        Some(ExpansionType::BackwardsToMatchPattern)
    } else if selection_only_preceding && replacement_has_primary {
        // Selected only comments/attributes but replacing with full logical unit
        Some(ExpansionType::ForwardToIncludePrimary)
    } else {
        // No expansion needed
        None
    }
}

/// Reconcile the target item's existing leading decorations (comments / attributes)
/// against the decorations the replacement supplies, returning the node set that
/// should be replaced — the target decorations *from the topmost shared one
/// downward*, plus the originally-selected nodes — or `None` when nothing should be
/// consumed.
///
/// The rule: find the topmost target decoration whose identity also appears in the
/// replacement (the *anchor*). From the anchor down, the replacement is
/// authoritative, so those decorations are consumed and replaced; decorations
/// *above* the anchor are left untouched. If the replacement shares no decoration
/// with the target, consume nothing — the existing decorations are preserved and
/// the replacement's are simply added. This is why
/// `#[derive(Clone)] pub struct …` over `#[derive(Debug)] /// doc pub struct …`
/// replaces the derive but, when the shared `derive` is the topmost match, keeps
/// whatever sits above it.
fn reconcile_backward<'a>(
    edit_nodes: &[Node<'a>],
    replacement: &[Node<'_>],
    target_src: &str,
    replacement_src: &str,
) -> Option<Vec<Node<'a>>> {
    let first = edit_nodes.first()?;

    // Target's contiguous leading decorations: walk siblings backward (nearest
    // first), then flip to top-to-bottom source order.
    let mut decorations = Vec::new();
    let mut current = *first;
    while let Some(prev) = current.prev_sibling() {
        if is_preceding_item(prev) {
            decorations.push(prev);
            current = prev;
        } else {
            break;
        }
    }
    if decorations.is_empty() {
        return None;
    }
    decorations.reverse();

    // Identities the replacement re-states.
    let replacement_keys: Vec<String> = replacement
        .iter()
        .take_while(|node| is_preceding_item(**node))
        .map(|node| decoration_key(*node, replacement_src))
        .collect();

    // Topmost target decoration the replacement also has — the anchor.
    let anchor = decorations
        .iter()
        .position(|node| replacement_keys.contains(&decoration_key(*node, target_src)))?;

    let mut expanded: Vec<Node<'a>> = decorations[anchor..].to_vec();
    expanded.extend_from_slice(edit_nodes);
    Some(expanded)
}

/// A stable identity for a leading decoration, used to detect which decorations the
/// replacement re-states. Attributes are keyed by their path (`derive`, `serde`, …)
/// so `#[derive(Debug)]` and `#[derive(Debug, Clone)]` are the same slot; any doc
/// comment is a single `doc` slot (so changing its text still matches); ordinary
/// comments are keyed by exact text.
fn decoration_key(node: Node<'_>, source: &str) -> String {
    let text = source
        .get(node.start_byte()..node.end_byte())
        .unwrap_or("")
        .trim_start();
    match node.kind() {
        "attribute_item" => format!("attr:{}", attr_path(text)),
        "line_comment" | "block_comment"
            if text.starts_with("///")
                || text.starts_with("//!")
                || text.starts_with("/**")
                || text.starts_with("/*!") =>
        {
            "doc".to_string()
        }
        _ => format!("text:{text}"),
    }
}

/// Extract an attribute's leading path from its source text, e.g. `derive` from
/// `#[derive(Debug)]` or `tokio::test` from `#[tokio::test]`.
fn attr_path(text: &str) -> &str {
    let rest = text
        .trim_start_matches('#')
        .trim_start_matches('!')
        .trim_start()
        .strip_prefix('[')
        .unwrap_or(text)
        .trim_start();
    let end = rest
        .find(|c: char| matches!(c, '(' | '[' | '{' | '=' | ']') || c.is_whitespace())
        .unwrap_or(rest.len());
    rest[..end].trim()
}

fn expand_forward_to_include_primary<'a>(edit_nodes: &[Node<'a>]) -> Option<Vec<Node<'a>>> {
    let last_node = edit_nodes.last()?;
    let mut expanded = edit_nodes.to_vec();
    let mut current = *last_node;

    // Walk forward until we find a primary item
    while let Some(next) = current.next_sibling() {
        expanded.push(next);
        if !is_preceding_item(next) {
            break; // Found the primary item
        }
        current = next;
    }

    Some(expanded)
}

fn apply_expansion<'editor, 'language>(
    edit: &mut Edit<'editor, 'language>,
    expanded_nodes: Vec<Node<'editor>>,
    expansion_type: ExpansionType,
) {
    // An `insert_after` edit's position is an end-anchored insertion point
    // (`start_byte == end_byte == node.end`, see `Edit::insert_after`). Expanding
    // the selection to group preceding doc-comments/attributes must keep that point
    // at the *end* of the group (after the primary's `}`), not drag it back to the
    // group's start — otherwise "insert after the function" silently inserts before
    // its doc-comment. `insert_before` and `replace` anchor on the group's start.
    let operation = edit.operation();
    let position = edit.position_mut();

    if let (Some(first), Some(last)) = (expanded_nodes.first(), expanded_nodes.last()) {
        if matches!(operation, Operation::InsertAfter) {
            position.set_start_byte(last.end_byte());
        } else {
            position.set_start_byte(first.start_byte());
            if position.end_byte.is_some() {
                position.end_byte = Some(last.end_byte());
            }
        }
    }

    let annotation = match expansion_type {
        ExpansionType::BackwardsToMatchPattern => "rust: expanded backwards to match pattern",
        ExpansionType::ForwardToIncludePrimary => "rust: expanded forward to include primary",
    };

    edit.set_annotation(annotation).set_nodes(expanded_nodes);
}

fn is_preceding_item(node: Node<'_>) -> bool {
    matches!(
        node.kind(),
        "line_comment" | "block_comment" | "attribute_item"
    )
}
