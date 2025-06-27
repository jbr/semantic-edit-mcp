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
    temporary_tracker: bool,
    staged_edit: Option<&'editor EditPosition>,
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
            temporary_tracker: false,
            selector,
            source_code,
            tree,
            staged_edit: staged_edit.as_ref(),
        }
    }

    fn find_edit_position(&self) -> Result<EditPosition, String> {
        let text_ranges = self.find_text_ranges()?;
        match &text_ranges[..] {
            [] => Err("No valid text ranges found".to_string()),
            [text_range] => Ok(text_range.into()),
            text_ranges => Err(format_multiple_matches(text_ranges, self.source_code)),
        }
    }

    fn find_text_ranges(&self) -> Result<Vec<TextRange>, String> {
        let selector: &Selector = self.selector;
        let source_code: &str = self.source_code;
        let tree: &Tree = self.tree;
        selector.validate()?;

        match selector {
            Selector::Insert { anchor, position } => {
                find_insert_positions(anchor, *position, source_code)
                    .map(|positions| positions.into_iter().map(TextRange::Insert).collect())
            }
            Selector::Replace { exact, from, to } => {
                if let Some(exact_text) = exact {
                    find_exact_matches(exact_text, source_code)
                        .map(|ranges| ranges.into_iter().map(TextRange::Replace).collect())
                } else if let Some(from_text) = from {
                    find_range_matches(from_text, to.as_deref(), source_code, tree)
                        .map(|ranges| ranges.into_iter().map(TextRange::Replace).collect())
                } else {
                    // This should be caught by validate(), but just in case
                    Err("Invalid replace operation".to_string())
                }
            }
        }
    }
}

impl<'editor, 'language> std::iter::Iterator for EditIterator<'editor, 'language> {
    type Item = Result<Edit<'editor, 'language>, String>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.temporary_tracker {
            return None;
        }

        self.temporary_tracker = true;

        if let Some(edit_position) = self.staged_edit {
            Some(Ok(Edit::new(self.editor, *edit_position)))
        } else {
            match self.find_edit_position() {
                Ok(edit_position) => Some(Ok(Edit::new(self.editor, edit_position))),
                Err(e) => Some(Err(e)),
            }
        }
    }
}

#[derive(Debug, Clone)]
enum TextRange {
    Insert(InsertTextPosition),
    Replace(ReplaceTextPosition),
}

#[derive(Debug, Clone)]
struct InsertTextPosition {
    byte_offset: usize,
    anchor: String,
    position: InsertPosition,
}

#[derive(Debug, Clone)]
struct ReplaceTextPosition {
    start_byte: usize,
    end_byte: usize,
    replace_type: ReplaceType,
}

#[derive(Debug, Clone)]
enum ReplaceType {
    Exact { text: String },
    Range { from: String, to: Option<String> },
}

impl From<TextRange> for EditPosition {
    fn from(value: TextRange) -> Self {
        match value {
            TextRange::Insert(InsertTextPosition { byte_offset, .. }) => Self {
                start_byte: byte_offset,
                end_byte: None,
            },
            TextRange::Replace(ReplaceTextPosition {
                start_byte,
                end_byte,
                ..
            }) => Self {
                start_byte,
                end_byte: Some(end_byte),
            },
        }
    }
}
impl From<&TextRange> for EditPosition {
    fn from(value: &TextRange) -> Self {
        match value {
            TextRange::Insert(InsertTextPosition { byte_offset, .. }) => Self {
                start_byte: *byte_offset,
                end_byte: None,
            },
            TextRange::Replace(ReplaceTextPosition {
                start_byte,
                end_byte,
                ..
            }) => Self {
                start_byte: *start_byte,
                end_byte: Some(*end_byte),
            },
        }
    }
}

impl TextRange {
    /// Get a human-readable description of this text range
    fn description(&self) -> String {
        match self {
            TextRange::Insert(insert) => {
                format!(
                    "Insert {} anchor \"{}\"",
                    match insert.position {
                        InsertPosition::Before => "before",
                        InsertPosition::After => "after",
                    },
                    insert.anchor
                )
            }
            TextRange::Replace(replace) => match &replace.replace_type {
                ReplaceType::Exact { text } => {
                    format!("Replace exact match \"{text}\"")
                }
                ReplaceType::Range { from, to } => {
                    if let Some(to_text) = to {
                        format!("Replace range from \"{from}\" to \"{to_text}\"")
                    } else {
                        format!("Replace from \"{from}\" (structural)")
                    }
                }
            },
        }
    }
}

fn find_insert_positions(
    anchor: &str,
    position: InsertPosition,
    source_code: &str,
) -> Result<Vec<InsertTextPosition>, String> {
    log::trace!("top of find_insert_positions for {anchor:?}, {position:?}");

    let positions = source_code
        .match_indices(anchor)
        .map(|(byte_offset, _)| {
            let adjusted_offset = match position {
                InsertPosition::Before => byte_offset,
                InsertPosition::After => byte_offset + anchor.len(),
            };
            InsertTextPosition {
                byte_offset: adjusted_offset,
                anchor: anchor.to_string(),
                position,
            }
        })
        .collect::<Vec<_>>();

    if positions.is_empty() {
        Err(format!("Anchor text \"{anchor}\" not found in source"))
    } else {
        Ok(positions)
    }
}

fn find_exact_matches(
    exact_text: &str,
    source_code: &str,
) -> Result<Vec<ReplaceTextPosition>, String> {
    let positions = source_code
        .match_indices(exact_text)
        .map(|(start_byte, matched)| ReplaceTextPosition {
            start_byte,
            end_byte: start_byte + matched.len(),
            replace_type: ReplaceType::Exact {
                text: exact_text.to_string(),
            },
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
) -> Result<Vec<ReplaceTextPosition>, String> {
    let from_positions: Vec<_> = source_code.match_indices(from_text).collect();

    if from_positions.is_empty() {
        return Err(format!("From text \"{from_text}\" not found in source"));
    }

    if let Some(to_text) = to_text {
        // Explicit range: find from...to pairs
        let to_positions: Vec<_> = source_code.match_indices(to_text).collect();

        if to_positions.is_empty() {
            return Err(format!("To text \"{to_text}\" not found in source"));
        }

        let mut ranges = Vec::new();

        for (from_byte, _) in from_positions {
            // Find the first 'to' position that comes after this 'from' position
            // Use outer edges: start of 'from' to end of 'to'
            for (to_byte, _) in &to_positions {
                if *to_byte >= from_byte + from_text.len() {
                    let start_byte = from_byte;
                    let end_byte = *to_byte + to_text.len();

                    ranges.push(ReplaceTextPosition {
                        start_byte,
                        end_byte,
                        replace_type: ReplaceType::Range {
                            from: from_text.to_string(),
                            to: Some(to_text.to_string()),
                        },
                    });
                    break; // Take the first valid 'to' for this 'from'
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
    } else {
        Ok(from_positions
            .into_iter()
            .filter_map(|(from, from_text)| {
                let from_end = from + from_text.len();
                tree.root_node()
                    .named_descendant_for_byte_range(from, from_end)
                    .or_else(|| tree.root_node().descendant_for_byte_range(from, from_end))
                    .map(|node| ReplaceTextPosition {
                        start_byte: node.start_byte(),
                        end_byte: node.end_byte(),
                        replace_type: ReplaceType::Exact {
                            text: source_code[node.byte_range()].to_string(),
                        },
                    })
            })
            .collect())
    }
}

fn get_context_around_byte_position(
    source_code: &str,
    byte_pos: usize,
    context_chars: usize,
) -> String {
    let start = byte_pos.saturating_sub(context_chars);
    let end = (byte_pos + context_chars).min(source_code.len());
    source_code[start..end]
        .replace('\n', "\\n")
        .replace('\t', "\\t")
}

fn format_multiple_matches(ranges: &[TextRange], source_code: &str) -> String {
    let mut message = format!(
        "Found {} possible matches. Please be more specific:\n\n",
        ranges.len()
    );

    for (i, range) in ranges.iter().enumerate() {
        match range {
            TextRange::Insert(insert) => {
                let context = get_context_around_byte_position(source_code, insert.byte_offset, 50);
                message.push_str(&format!(
                    "{}. Insert {} anchor \"{}\": {}\n",
                    i + 1,
                    insert.position,
                    insert.anchor,
                    context
                ));
            }
            TextRange::Replace(replace) => {
                let text = &source_code[replace.start_byte..replace.end_byte];
                let preview = if text.len() > 100 {
                    format!("{}...", &text[..97])
                } else {
                    text.to_string()
                };
                message.push_str(&format!(
                    "{}. {}: {}\n",
                    i + 1,
                    range.description(),
                    preview.replace('\n', "\\n")
                ));
            }
        }
    }

    message.push_str(
        "\nSuggestion: Add more context to your anchor text to uniquely identify the target.",
    );
    message
}
