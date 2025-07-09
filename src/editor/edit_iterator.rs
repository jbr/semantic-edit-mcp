use std::{borrow::Cow, iter::Iterator};

use tree_sitter::Tree;

use crate::{
    editor::EditPosition,
    selector::{Operation, Selector},
};

use super::{Edit, Editor};

#[derive(fieldwork::Fieldwork)]
#[fieldwork(get, into)]
pub struct EditIterator<'editor, 'language> {
    editor: &'editor Editor<'language>,
    #[fieldwork(with, get_mut, set)]
    selector: Cow<'editor, Selector>,
    source_code: &'editor str,
    #[fieldwork(with, get_mut, set)]
    content: Cow<'editor, str>,
    tree: &'editor Tree,
    #[fieldwork(skip)]
    staged_edit: Option<&'editor EditPosition>,
    #[fieldwork(get_mut(deref = false))]
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
            additional.push(edit.clone().with_content(format!(" {}", &edit.content)));
            additional.push(edit.clone().with_content(format!("\n{}", &edit.content)));
        }
        edits.extend_from_slice(&additional);
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
            additional.push(edit.clone().with_content(format!("{}\n", &edit.content)));
            additional.push(edit.clone().with_content(format!("{} ", &edit.content)));
        }
        edits.extend_from_slice(&additional);
        Ok(edits)
    }

    fn select_ast_node(
        &self,
        anchor: &str,
        source_code: &str,
        tree: &'editor Tree,
    ) -> Result<Vec<Edit<'editor, 'language>>, String> {
        let anchor = anchor.trim().lines().next().unwrap_or_default();

        Ok(from_positions(source_code, anchor.trim())?
            .into_iter()
            .flat_map(|(from, anchor)| {
                let from_end = from + anchor.len();
                let mut candidates = Vec::new();

                if let Some(node) = tree
                    .root_node()
                    .named_descendant_for_byte_range(from, from_end)
                    .or_else(|| tree.root_node().descendant_for_byte_range(from, from_end))
                {
                    // Original node
                    candidates.push(
                        self.build_edit(node.start_byte())
                            .with_end_byte(node.end_byte())
                            .with_node(node),
                    );

                    // Parent node (if exists and different)
                    if let Some(parent) = node.parent() {
                        candidates.push(
                            self.build_edit(parent.start_byte())
                                .with_end_byte(parent.end_byte())
                                .with_node(parent),
                        );

                        // Grandparent node (if exists and different)
                        if let Some(grandparent) = parent.parent() {
                            candidates.push(
                                self.build_edit(grandparent.start_byte())
                                    .with_end_byte(grandparent.end_byte())
                                    .with_node(parent),
                            );
                        }
                    }
                }
                candidates
            })
            .collect())
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

fn from_positions<'a>(source_code: &'a str, anchor: &str) -> Result<Vec<(usize, &'a str)>, String> {
    let mut from_positions: Vec<_> = source_code.match_indices(anchor.trim()).collect();
    if from_positions.is_empty() {
        if let Some(first_line) = anchor.lines().next() {
            from_positions.extend(source_code.match_indices(first_line.trim()));
        }
    }

    if from_positions.is_empty() {
        return Err(format!("From text \"{anchor}\" not found in source"));
    }
    Ok(from_positions)
}
