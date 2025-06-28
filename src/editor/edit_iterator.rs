use std::iter::Iterator;

use tree_sitter::Tree;

use crate::{
    editor::EditPosition,
    selector::{Operation, Selector},
};

use super::{Edit, Editor};

pub(super) struct EditIterator<'editor, 'language> {
    editor: &'editor Editor<'language>,
    selector: &'editor Selector,
    source_code: &'editor str,
    tree: &'editor Tree,
    staged_edit: Option<&'editor EditPosition>,
    positions: Option<Vec<EditPosition>>,
    current_index: usize,
}

impl<'editor, 'language> EditIterator<'editor, 'language> {
    pub(crate) fn new(editor: &'editor Editor<'language>) -> Self {
        let Editor {
            selector,
            source_code,
            tree,
            staged_edit,
            ..
        } = &editor;
        Self {
            editor,
            selector,
            source_code,
            tree,
            staged_edit: staged_edit.as_ref(),
            positions: None,
            current_index: 0,
        }
    }

    fn find_text_ranges(&self) -> Result<Vec<EditPosition>, String> {
        let source_code: &str = self.source_code;
        let tree: &Tree = self.tree;
        self.selector.validate()?;
        let Selector {
            operation,
            anchor,
            end,
        } = &self.selector;

        match operation {
            Operation::InsertBefore => find_insert_positions(anchor, true, source_code),
            Operation::InsertAfter => find_insert_positions(anchor, false, source_code),
            Operation::ReplaceRange => find_range_matches(anchor, end.as_deref(), source_code),
            Operation::ReplaceExact => find_exact_matches(anchor, source_code),
            Operation::ReplaceNode => select_ast_node(anchor, source_code, tree),
            Operation::InsertAfterNode => {
                find_after_ast_insert_positions(anchor, source_code, tree)
            }
        }
    }

    fn ensure_text_ranges_loaded(&mut self) -> Result<(), String> {
        if self.positions.is_none() {
            self.positions = Some(self.find_text_ranges()?);
        }
        Ok(())
    }
}

fn find_after_ast_insert_positions(
    anchor: &str,
    source_code: &str,
    tree: &Tree,
) -> Result<Vec<EditPosition>, String> {
    Ok(select_ast_node(anchor, source_code, tree)?
        .into_iter()
        .filter_map(|edit_position| {
            edit_position.end_byte.map(|start_byte| EditPosition {
                start_byte,
                end_byte: None,
            })
        })
        .collect())
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
        let text_ranges = self.positions.as_ref().unwrap();
        if self.current_index >= text_ranges.len() {
            return None; // No more ranges to try
        }

        let edit_position = text_ranges[self.current_index];
        self.current_index += 1;

        Some(Ok(Edit::new(self.editor, edit_position)))
    }
}

fn find_insert_positions(
    anchor: &str,
    before: bool,
    source_code: &str,
) -> Result<Vec<EditPosition>, String> {
    let positions = source_code
        .match_indices(anchor)
        .map(|(byte_offset, _)| EditPosition {
            start_byte: if before {
                byte_offset
            } else {
                byte_offset + anchor.len()
            },
            end_byte: None,
        })
        .collect::<Vec<_>>();

    if positions.is_empty() {
        Err(format!("Anchor text \"{anchor}\" not found in source"))
    } else {
        Ok(positions)
    }
}

fn find_exact_matches(exact_text: &str, source_code: &str) -> Result<Vec<EditPosition>, String> {
    let positions = source_code
        .match_indices(exact_text)
        .map(|(start_byte, matched)| EditPosition {
            start_byte,
            end_byte: Some(start_byte + matched.len()),
        })
        .collect::<Vec<_>>();

    if positions.is_empty() {
        Err(format!("Exact text \"{exact_text}\" not found in source"))
    } else {
        Ok(positions)
    }
}

fn find_range_matches(
    anchor: &str,
    end: Option<&str>,
    source_code: &str,
) -> Result<Vec<EditPosition>, String> {
    if let Some(end) = end {
        find_explicit_range(anchor, end, source_code)
    } else {
        Err("end is required for range replacement".to_string())
    }
}

fn select_ast_node(
    anchor: &str,
    source_code: &str,
    tree: &Tree,
) -> Result<Vec<EditPosition>, String> {
    Ok(from_positions(source_code, anchor)?
        .into_iter()
        .filter_map(|(from, anchor)| {
            let from_end = from + anchor.len();
            tree.root_node()
                .named_descendant_for_byte_range(from, from_end)
                .or_else(|| tree.root_node().descendant_for_byte_range(from, from_end))
                .map(|node| EditPosition {
                    start_byte: node.start_byte(),
                    end_byte: Some(node.end_byte()),
                })
        })
        .collect())
}

fn from_positions<'a>(source_code: &'a str, anchor: &str) -> Result<Vec<(usize, &'a str)>, String> {
    let from_positions: Vec<_> = source_code.match_indices(anchor).collect();
    if from_positions.is_empty() {
        return Err(format!("From text \"{anchor}\" not found in source"));
    }
    Ok(from_positions)
}

fn to_positions<'a>(source_code: &'a str, end: &str) -> Result<Vec<(usize, &'a str)>, String> {
    let to_positions: Vec<_> = source_code.match_indices(end).collect();
    if to_positions.is_empty() {
        return Err(format!("To text \"{end}\" not found in source"));
    }
    Ok(to_positions)
}

fn find_explicit_range(
    anchor: &str,
    end: &str,
    source_code: &str,
) -> Result<Vec<EditPosition>, String> {
    let mut ranges = Vec::new();

    for (from_byte, _) in from_positions(source_code, anchor)? {
        for (to_byte, _) in to_positions(source_code, end)? {
            if to_byte >= from_byte + anchor.len() {
                ranges.push(EditPosition {
                    start_byte: from_byte,
                    end_byte: Some(to_byte + end.len()),
                });
            }
        }
    }

    if ranges.is_empty() {
        Err(format!(
            "No valid range found from \"{anchor}\" to \"{end}\""
        ))
    } else {
        Ok(ranges)
    }
}
