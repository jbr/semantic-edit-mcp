use anyhow::Result;
use ropey::Rope;
use tree_sitter::{InputEdit, Point, Tree};

use super::{EditPosition, Editor};

pub(super) struct Edit<'editor, 'language> {
    pub(super) editor: &'editor Editor<'language>,
    pub(super) tree: Tree,
    pub(super) rope: Rope,
    pub(super) position: EditPosition,
    pub(super) valid: bool,
    pub(super) message: Option<String>,
    pub(super) output: Option<String>,
}

impl<'editor, 'language> Edit<'editor, 'language> {
    pub fn new(editor: &'editor Editor<'language>, position: EditPosition) -> Self {
        Self {
            editor,
            tree: editor.tree.clone(),
            rope: editor.rope.clone(),
            position,
            valid: false,
            message: None,
            output: None,
        }
    }

    pub fn is_valid(&self) -> bool {
        self.valid
    }

    fn byte_to_point(&self, byte_idx: usize) -> Point {
        let line = self.rope.byte_to_line(byte_idx);
        let line_start_byte = self.rope.line_to_byte(line);
        let column = byte_idx - line_start_byte;

        Point { row: line, column }
    }

    pub(crate) fn apply(&mut self) -> Result<()> {
        let content = &self.editor.content;

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
            return Ok(());
        }

        if let Some(message) = self.validate(&output) {
            self.message = Some(message);
        } else {
            self.valid = true;
            self.message = Some(format!(
                "Applied {} operation",
                self.editor.selector.operation_name()
            ));

            self.output = Some(self.editor.format_code(&output)?);
        }

        Ok(())
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

    pub(crate) fn message(&mut self) -> String {
        self.message.take().unwrap_or_default()
    }

    pub(crate) fn output(&mut self) -> Option<String> {
        self.output.take()
    }
}
