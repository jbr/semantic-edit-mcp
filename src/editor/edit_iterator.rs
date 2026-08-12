use std::{borrow::Cow, iter::Iterator};

use tree_sitter::{Node, Tree};

use crate::{
    editor::EditPosition,
    searcher::find_positions,
    selector::{Operation, Selector},
};

use super::{Edit, Editor};

#[derive(fieldwork::Fieldwork)]
#[fieldwork(get, into)]
pub struct EditIterator<'editor, 'language> {
    editor: &'editor Editor<'language>,
    #[field(with, get_mut, set)]
    selector: Cow<'editor, Selector>,
    source_code: &'editor str,
    #[field(with, get_mut, set)]
    content: Cow<'editor, str>,
    tree: &'editor Tree,
    #[field(get_mut(deref = false))]
    edits: Option<Vec<Edit<'editor, 'language>>>,
    current_index: usize,
}

impl<'editor, 'language> EditIterator<'editor, 'language> {
    pub(crate) fn new(editor: &'editor Editor<'language>) -> Self {
        let Editor {
            selector,
            source_code,
            tree,
            content,
            ..
        } = &editor;
        Self {
            editor,
            selector: Cow::Borrowed(selector),
            content: Cow::Borrowed(content),
            source_code,
            tree,
            edits: None,
            current_index: 0,
        }
    }

    pub(crate) fn find_edits(&self) -> Result<Vec<Edit<'editor, 'language>>, String> {
        let source_code: &str = self.source_code;
        let tree: &Tree = self.tree;
        self.selector.validate()?;
        let Selector { operation, anchor } = &*self.selector;

        match operation {
            Operation::InsertAfter => {
                self.find_after_ast_insert_positions(anchor, source_code, tree)
            }
            Operation::InsertBefore => {
                self.find_before_ast_insert_positions(anchor, source_code, tree)
            }
            Operation::Replace => self.select_ast_node(anchor, source_code, tree),
        }
    }

    fn ensure_text_ranges_loaded(&mut self) -> Result<(), String> {
        if self.edits.is_none() {
            self.edits = Some(self.find_edits()?);
        }
        Ok(())
    }

    fn build_edit(&self, start_byte: usize) -> Edit<'editor, 'language> {
        Edit::new(
            self.editor,
            EditPosition {
                start_byte,
                end_byte: None,
            },
        )
        .with_content(self.content.clone())
    }

    fn find_after_ast_insert_positions(
        &self,
        anchor: &str,
        source_code: &str,
        tree: &'editor Tree,
    ) -> Result<Vec<Edit<'editor, 'language>>, String> {
        let mut edits = self
            .select_ast_node(anchor, source_code, tree)?
            .into_iter()
            .filter_map(Edit::insert_after)
            .collect::<Vec<_>>();

        let mut additional = vec![];
        for edit in &edits {
            additional.push(edit.clone().with_content(format!(" {}", edit.content())));
            additional.push(edit.clone().with_content(format!("\n{}", edit.content())));
        }
        edits.extend(additional);
        Ok(edits)
    }
    fn find_before_ast_insert_positions(
        &self,
        anchor: &str,
        source_code: &str,
        tree: &'editor Tree,
    ) -> Result<Vec<Edit<'editor, 'language>>, String> {
        let mut edits = self
            .select_ast_node(anchor, source_code, tree)?
            .into_iter()
            .map(Edit::insert_before)
            .collect::<Vec<_>>();

        let mut additional = vec![];
        for edit in &edits {
            additional.push(edit.clone().with_content(format!("{}\n", edit.content())));
            additional.push(edit.clone().with_content(format!("{} ", edit.content())));
        }
        edits.extend(additional);
        Ok(edits)
    }

    fn select_ast_node(
        &self,
        anchor: &str,
        source_code: &str,
        tree: &'editor Tree,
    ) -> Result<Vec<Edit<'editor, 'language>>, String> {
        let anchor = anchor.trim();
        let is_insert = matches!(
            self.selector.operation,
            Operation::InsertAfter | Operation::InsertBefore
        );
        let mut candidates = vec![];
        let mut anchor_found = false;
        for (start, end) in find_positions(source_code, anchor)? {
            anchor_found = true;
            if let Some(mut parent) = tree.root_node().descendant_for_byte_range(start, end) {
                // An anchor matching inside a comment resolves to the comment's
                // inner trivia (e.g. rust's `///` marker / `doc_comment` text
                // tokens); climb to the comment node itself so candidates target
                // it whole rather than splicing mid-token.
                while let Some(outer) = parent.parent().filter(|outer| outer.is_extra()) {
                    parent = outer;
                }
                // For the same reason, never descend *into* a comment for the
                // node-range candidate: its children are sub-token trivia, and an
                // edit relative to them lands inside the comment's own text
                // (`//<content>/ rest-of-comment`).
                let nodes = if parent.is_extra() {
                    vec![]
                } else {
                    siblings_in_range(parent, start, end)
                };
                // A node-set candidate must be *boundary-aligned* with the anchor,
                // or it targets something other than what the anchor names:
                //
                // - **replace** targets "the node that starts with the anchor", so
                //   the first node must start exactly at the anchor — or the anchor
                //   may land past the node's leading modifiers but still in its
                //   header (`fn alpha` matching inside `pub async fn alpha`). An
                //   anchor that *begins* inside a node's body (e.g. sloppily
                //   including the previous item's closing `}`) partially covers a
                //   neighbor; replacing the whole node set would silently consume
                //   code the anchor never named.
                // - **inserts** target "the node containing the anchor", so the
                //   node set must contain the whole anchor span. A partial overlap
                //   (anchor `pub fn` covering just the `pub` modifier) would place
                //   the insertion *inside* the item it was meant to be adjacent to.
                //   And when the node set's insertion boundary is the same byte as
                //   the parent's own boundary, the node-range candidate is
                //   redundant — but *worse*: its nodes are the parent's children,
                //   so language-specific grouping can't see the parent's siblings
                //   (a struct's preceding `#[derive]` / doc comments) and the
                //   insertion splits them from the item they decorate. Skip it and
                //   let the common-parent candidate, which carries the whole node,
                //   handle that position.
                if let (Some(first), Some(last)) = (nodes.first(), nodes.last()) {
                    let aligned = if is_insert {
                        first.start_byte() <= start
                            && last.end_byte() >= end
                            && match self.selector.operation {
                                Operation::InsertBefore => {
                                    first.start_byte() != parent.start_byte()
                                }
                                Operation::InsertAfter => last.end_byte() != parent.end_byte(),
                                Operation::Replace => unreachable!(),
                            }
                    } else {
                        first.start_byte() == start || anchor_in_header(*first, start)
                    };
                    if aligned {
                        candidates.push(
                            self.build_edit(first.start_byte())
                                .with_end_byte(last.end_byte())
                                .with_anchor_hit((start, end))
                                .with_nodes(nodes)
                                .with_annotation("node range"),
                        );
                    }
                }

                // The "common parent" candidate inserts/replaces relative to the
                // whole `parent`. For an **insert**, when `parent` begins before the
                // anchor (`parent.start < start`), the anchor sits *interior* to
                // `parent`, so inserting after/before `parent` would escape a
                // container the anchor was merely inside (e.g. dropping a new
                // interface member after the closing `}`). Skip it in that case so
                // the in-container candidates win or the edit fails safe, instead of
                // a structurally-distant placement coincidentally parsing.
                //
                // BUT an anchor can also sit interior to its *own* target node by
                // landing in that node's header rather than its body — `fn alpha`
                // matches past the `pub async ` modifiers of `pub async fn alpha`, so
                // `parent` (the `function_item`) starts before the anchor even though
                // it *is* the node we mean to target. Only treat the anchor as
                // escaping when it falls inside the parent's `body` (after the
                // signature); an anchor in the header keeps the common-parent
                // candidate so we don't fall back to splitting the signature.
                //
                // **replace** applies the same boundary-alignment rule as the
                // node-range candidate: exact start, or a header-interior anchor.
                let keep = if is_insert {
                    !(parent.start_byte() < start && !anchor_in_header(parent, start))
                } else {
                    parent.start_byte() == start || anchor_in_header(parent, start)
                };
                if keep {
                    candidates.push(
                        self.build_edit(parent.start_byte())
                            .with_end_byte(parent.end_byte())
                            .with_anchor_hit((start, end))
                            .with_nodes(vec![parent])
                            .with_annotation("common parent"),
                    );
                }
            }

            // The exact-byte-range candidate is the *lexical* fallback, so its
            // guard is lexical where the node candidates' guards are structural.
            // For inserts it is the placement of last resort (e.g. dropping the
            // first statement into an empty body right after the `{` the anchor
            // ends on), and a bad splice almost always fails to re-parse. For
            // **replace** it serves two textual idioms, and is gated to them:
            //
            // - a *sub-token tweak* — swapping part of a string literal or
            //   comment — recognized by both span endpoints lying in the same
            //   named node;
            // - an *old_string-style straddle* — rewriting a run of text that
            //   needn't align with node boundaries (a derive plus the struct
            //   header below it; a TOML entry plus the next table's header) —
            //   recognized by the span being whitespace-delimited on both sides.
            //
            // What this rejects is a span glued to its lexical surroundings, like
            // `users.insert` inside `self.users.insert(…)`: splicing replacement
            // text between `self.` and `(` fabricates expressions the author
            // never wrote, and such results too often *still parse*, turning a
            // mistargeted anchor into silently mangled code.
            let root = tree.root_node();
            let sub_token_tweak = end > start
                && root.named_descendant_for_byte_range(start, start + 1)
                    == root.named_descendant_for_byte_range(end - 1, end);
            let whitespace_delimited = source_code[..start]
                .chars()
                .next_back()
                .is_none_or(char::is_whitespace)
                && source_code[end..]
                    .chars()
                    .next()
                    .is_none_or(char::is_whitespace);
            let exact_ok = is_insert || sub_token_tweak || whitespace_delimited;
            if exact_ok {
                candidates.push(
                    self.build_edit(start)
                        .with_end_byte(end)
                        .with_anchor_hit((start, end))
                        .with_annotation("exact"),
                );
            }
        }

        if candidates.is_empty() && anchor_found {
            return Err(format!(
                "The anchor {anchor:?} was found, but it does not line up with the boundaries \
of any complete syntax node — it starts or ends partway through a node (for example, by \
including a closing brace or other text from a neighboring item). No change was performed.\n\
Anchor the text of the node you mean to target, starting from its first token.",
            ));
        }

        Ok(candidates)
    }
}

impl<'editor, 'language> Iterator for EditIterator<'editor, 'language> {
    type Item = Result<Edit<'editor, 'language>, String>;

    fn next(&mut self) -> Option<Self::Item> {
        // Ensure text ranges are loaded
        if let Err(e) = self.ensure_text_ranges_loaded() {
            return Some(Err(e));
        }

        // Get the current text range to try
        let text_ranges = self.edits.as_ref().unwrap();
        if self.current_index >= text_ranges.len() {
            return None; // No more ranges to try
        }

        let edit = text_ranges[self.current_index].clone();
        self.current_index += 1;

        Some(Ok(edit))
    }
}

/// Whether an anchor starting at `start` lands in `node`'s *header* — at or after
/// the node's own start but before its `body` field begins (`fn alpha` matching
/// past the modifiers of `pub async fn alpha`). Nodes without a `body` field have
/// no header, so an interior anchor is never header-interior for them.
fn anchor_in_header(node: Node<'_>, start: usize) -> bool {
    node.child_by_field_name("body")
        .is_some_and(|body| start < body.start_byte())
}

fn siblings_in_range<'tree>(parent: Node<'tree>, start: usize, end: usize) -> Vec<Node<'tree>> {
    // Collect all named children that intersect the range
    let mut result = Vec::new();
    let mut cursor = parent.walk();

    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if child.is_named() && child.start_byte() < end && child.end_byte() > start {
                result.push(child);
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    result
}
