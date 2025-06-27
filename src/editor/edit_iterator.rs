use std::iter::Iterator;

use tree_sitter::Tree;

use crate::{
    editor::EditPosition,
    selector::{InsertPosition, Selector},
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
        let selector: &Selector = self.selector;
        let source_code: &str = self.source_code;
        let tree: &Tree = self.tree;
        selector.validate()?;

        match selector {
            Selector::Insert { anchor, position } => {
                find_insert_positions(anchor, *position, source_code)
            }
            Selector::Replace { exact, from, to } => {
                if let Some(exact_text) = exact {
                    find_exact_matches(exact_text, source_code)
                } else if let Some(from_text) = from {
                    find_range_matches(from_text, to.as_deref(), source_code, tree)
                } else {
                    // This should be caught by validate(), but just in case
                    Err("Invalid replace operation".to_string())
                }
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
    position: InsertPosition,
    source_code: &str,
) -> Result<Vec<EditPosition>, String> {
    log::trace!("top of find_insert_positions for {anchor:?}, {position:?}");

    let positions = source_code
        .match_indices(anchor)
        .map(|(byte_offset, _)| EditPosition {
            start_byte: match position {
                InsertPosition::Before => byte_offset,
                InsertPosition::After => byte_offset + anchor.len(),
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
    from_text: &str,
    to_text: Option<&str>,
    source_code: &str,
    tree: &Tree,
) -> Result<Vec<EditPosition>, String> {
    if let Some(to_text) = to_text {
        find_explicit_range(from_text, to_text, source_code)
    } else {
        select_ast_node(from_text, source_code, tree)
    }
}

fn select_ast_node(
    from_text: &str,
    source_code: &str,
    tree: &Tree,
) -> Result<Vec<EditPosition>, String> {
    Ok(from_positions(source_code, from_text)?
        .into_iter()
        .filter_map(|(from, from_text)| {
            let from_end = from + from_text.len();
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

fn from_positions<'a>(
    source_code: &'a str,
    from_text: &str,
) -> Result<Vec<(usize, &'a str)>, String> {
    let from_positions: Vec<_> = source_code.match_indices(from_text).collect();
    if from_positions.is_empty() {
        return Err(format!("From text \"{from_text}\" not found in source"));
    }
    Ok(from_positions)
}

fn to_positions<'a>(source_code: &'a str, to_text: &str) -> Result<Vec<(usize, &'a str)>, String> {
    let to_positions: Vec<_> = source_code.match_indices(to_text).collect();
    if to_positions.is_empty() {
        return Err(format!("To text \"{to_text}\" not found in source"));
    }
    Ok(to_positions)
}

fn find_explicit_range(
    from_text: &str,
    to_text: &str,
    source_code: &str,
) -> Result<Vec<EditPosition>, String> {
    let mut ranges = Vec::new();

    for (from_byte, _) in from_positions(source_code, from_text)? {
        for (to_byte, _) in to_positions(source_code, to_text)? {
            if to_byte >= from_byte + from_text.len() {
                ranges.push(EditPosition {
                    start_byte: from_byte,
                    end_byte: Some(to_byte + to_text.len()),
                });
            }
        }
    }

    if ranges.is_empty() {
        Err(format!(
            "No valid range found from \"{from_text}\" to \"{to_text}\""
        ))
    } else {
        Ok(ranges)
    }
}
