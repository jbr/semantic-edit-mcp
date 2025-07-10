use crate::editor::edit_iterator::find_positions;
use anyhow::Result;
use fieldwork::Fieldwork;
use ropey::Rope;
use std::{
    borrow::Cow,
    fmt::{self, Debug, Formatter},
};
use tree_sitter::{InputEdit, Node, Point, Tree};

use super::{EditPosition, Editor};

#[derive(Clone, Fieldwork)]
#[fieldwork(option_set_some)]
pub struct Edit<'editor, 'language> {
    pub(super) editor: &'editor Editor<'language>,
    pub(super) tree: Tree,
    pub(super) rope: Rope,
    #[field(get, set, with, get_mut(deref = false), into)]
    pub(super) content: Cow<'editor, str>,
    #[field(get, get_mut)]
    pub(super) position: EditPosition,
    #[field(get = is_valid)]
    pub(super) valid: Option<bool>,
    #[field(get, take)]
    pub(super) message: Option<String>,
    #[field(get, take)]
    pub(super) output: Option<String>,
    #[field(get, set, with, take)]
    pub(super) node: Option<Node<'editor>>,
    #[field(with, get, set)]
    internal_explanation: Option<&'static str>,
}

impl<'editor, 'language> Debug for Edit<'editor, 'language> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let mut s = f.debug_struct("Edit");
        s.field("content", &self.content)
            .field("anchor", &self.editor.selector.anchor)
            .field("start_byte", &self.position.start_byte);
        if let Some(end_byte) = self.position.end_byte {
            s.field("end_byte", &end_byte);
        }
        s.field("valid", &self.valid);
        if let Some(node) = self.node {
            s.field("node_kind", &node.kind());
        }
        if let Some(edit_region) = self.edit_region() {
            s.field("edit_region", &edit_region);
        }
        if let Some(explanation) = self.internal_explanation {
            s.field("internal_explanation", &explanation);
        }
        s.finish()
    }
}

impl<'editor, 'language> Edit<'editor, 'language> {
    pub fn new(editor: &'editor Editor<'language>, position: EditPosition) -> Self {
        Self {
            editor,
            tree: editor.tree.clone(),
            rope: editor.rope.clone(),
            position,
            content: Cow::Borrowed(&editor.content),
            valid: None,
            message: None,
            output: None,
            node: None,
            internal_explanation: None,
        }
    }

    pub fn insert_before(mut self) -> Self {
        if let Some(edit_region) = self.edit_region() {
            if let Ok(positions) = find_positions(&self.content, edit_region) {
                if let Some((start, _)) = positions.last() {
                    match &mut self.content {
                        Cow::Borrowed(borrowed) => *borrowed = &borrowed[..*start],
                        Cow::Owned(owned) => *owned = owned[..*start].to_string(),
                    };
                }
            }
        }

        self.position.end_byte = None;
        self
    }

    pub fn insert_after(mut self) -> Option<Self> {
        let edit_region = self.edit_region()?;
        if let Ok(positions) = find_positions(&self.content, edit_region) {
            if let Some((_, end)) = positions.first() {
                match &mut self.content {
                    Cow::Borrowed(borrowed) => *borrowed = &borrowed[*end..],
                    Cow::Owned(owned) => *owned = owned[*end..].to_string(),
                };
            }
        }
        self.position.start_byte = self.position.end_byte.take()?;

        Some(self)
    }

    pub fn edit_region(&self) -> Option<&'editor str> {
        if let EditPosition {
            start_byte,
            end_byte: Some(end_byte),
        } = self.position
        {
            self.editor.source_code.get(start_byte..end_byte)
        } else {
            None
        }
    }

    pub fn with_end_byte(mut self, end_byte: usize) -> Self {
        self.position.end_byte = Some(end_byte);
        self
    }

    fn byte_to_point(&self, byte_idx: usize) -> Point {
        let line = self.rope.byte_to_line(byte_idx);
        let line_start_byte = self.rope.line_to_byte(line);
        let column = byte_idx - line_start_byte;

        Point { row: line, column }
    }

    pub(crate) fn apply(&mut self) -> Result<bool> {
        if let Some(valid) = self.valid {
            return Ok(valid);
        }

        let content = &self.content;

        let EditPosition {
            start_byte,
            end_byte,
        } = self.position;

        let start_char = self.rope.byte_to_char(start_byte);
        let start_position = self.byte_to_point(start_byte);

        let (old_end_byte, old_end_position) = if let Some(old_end_byte) = end_byte {
            let end_char = self.rope.byte_to_char(old_end_byte);
            let old_end_position = self.byte_to_point(old_end_byte);

            self.rope.remove(start_char..end_char);

            (old_end_byte, old_end_position)
        } else {
            (start_byte, start_position)
        };

        self.rope.insert(start_char, content);

        let new_end_byte = start_byte + content.len();
        let new_end_position = self.byte_to_point(new_end_byte);

        self.tree.edit(&InputEdit {
            start_byte,
            old_end_byte,
            new_end_byte,
            start_position,
            old_end_position,
            new_end_position,
        });

        let output = self.rope.to_string();

        if let Some(tree) = self.editor.parse(&output, Some(&self.tree)) {
            self.tree = tree;
        } else {
            self.message = Some("Unable to parse result so no changes were made. The file is still in a good state. Try a different edit".into());
            return Ok(false);
        }

        let valid = if let Some(message) = self.validate(&output) {
            self.message = Some(message);
            false
        } else {
            self.message = Some(format!(
                "Applied {} operation",
                self.editor.selector.operation_name()
            ));

            self.output = Some(self.editor.format_code(&output)?);
            true
        };

        self.valid = Some(valid);
        Ok(valid)
    }

    fn validate(&mut self, output: &str) -> Option<String> {
        let errors = self.editor.validate_tree(&self.tree, output)?;
        let diff = self.editor.diff(output);
        Some(format!(
            "This edit would result in invalid syntax, but the file is still in a valid state. \
No change was performed.
Suggestion: Try a different change.\n
{errors}\n\n{diff}"
        ))
    }

    pub(crate) fn source_code(&self) -> &'editor str {
        self.editor.source_code()
    }

    pub(crate) fn start_byte(&self) -> usize {
        self.position.start_byte
    }

    pub(crate) fn set_start_byte(&mut self, start_byte: usize) -> &mut Self {
        self.position.start_byte = start_byte;
        self
    }

    pub(crate) fn modify(mut fun: impl FnMut(&mut Self)) -> impl FnMut(Self) -> Self {
        move |mut edit| {
            fun(&mut edit);
            edit
        }
    }
}
