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
    #[field = false]
    staged_edit: Option<&'editor EditPosition>,
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
            staged_edit,
            content,
            ..
        } = &editor;
        Self {
            editor,
            selector: Cow::Borrowed(selector),
            content: Cow::Borrowed(content),
            source_code,
            tree,
            staged_edit: staged_edit.as_ref(),
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
            additional.push(edit.clone().with_content(format!(" {}", &edit.content())));
            additional.push(edit.clone().with_content(format!("\n{}", &edit.content())));
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
        let mut candidates = vec![];
        for (start, end) in find_positions(source_code, anchor)? {
            if let Some(parent) = tree.root_node().descendant_for_byte_range(start, end) {
                let nodes = siblings_in_range(parent, start, end);
                if !nodes.is_empty() {
                    candidates.push(
                        self.build_edit(nodes.first().as_ref().unwrap().start_byte())
                            .with_end_byte(nodes.last().as_ref().unwrap().end_byte())
                            .with_node(*nodes.first().unwrap())
                            .with_annotation("node range"),
                    );
                }

                candidates.push(
                    self.build_edit(parent.start_byte())
                        .with_end_byte(parent.end_byte())
                        .with_node(parent)
                        .with_annotation("common parent"),
                );
            }

            candidates.push(
                self.build_edit(start)
                    .with_end_byte(end)
                    .with_annotation("exact"),
            );
        }

        Ok(candidates)
    }
}

impl<'editor, 'language> Iterator for EditIterator<'editor, 'language> {
    type Item = Result<Edit<'editor, 'language>, String>;

    fn next(&mut self) -> Option<Self::Item> {
        // If we have a staged edit, return it first and only once
        if let Some(edit_position) = self.staged_edit.take() {
            return Some(Ok(Edit::new(self.editor, *edit_position)));
        }

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
