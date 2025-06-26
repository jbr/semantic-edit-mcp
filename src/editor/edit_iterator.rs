use super::{Edit, Editor};

pub(super) struct EditIterator<'editor, 'language> {
    editor: &'editor Editor<'language>,
    temporary_tracker: bool,
}

impl<'editor, 'language> EditIterator<'editor, 'language> {
    pub(crate) fn new(editor: &'editor Editor<'language>) -> Self {
        Self {
            editor,
            temporary_tracker: false,
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

        if let Some(edit_position) = self.editor.staged_edit {
            Some(Ok(Edit::new(self.editor, edit_position)))
        } else {
            match self
                .editor
                .selector
                .find_edit_position(&self.editor.source_code)
            {
                Ok(edit_position) => Some(Ok(Edit::new(self.editor, edit_position))),
                Err(e) => Some(Err(e)),
            }
        }
    }
}
