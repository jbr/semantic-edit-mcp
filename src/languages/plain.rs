use crate::{
    editor::{Edit, EditIterator, Editor},
    languages::{LanguageCommon, LanguageName, traits::LanguageEditor},
    layout::Rules,
};
use anyhow::Result;

pub fn language() -> LanguageCommon {
    LanguageCommon {
        name: LanguageName::Other,
        file_extensions: &[],
        language: tree_sitter_plain::LANGUAGE.into(),
        editor: Box::new(PlainEditor),
        validation_query: None,
    }
}

struct PlainEditor;

impl LanguageEditor for PlainEditor {
    /// Prose: content is always taken as written. A replacement's first line
    /// keeps the line's indentation unless it supplies its own, and continuation
    /// lines are never re-indented, since a list item's nesting or an indented
    /// code block is the text's meaning. An insert next to a whole line goes on a
    /// line of its own, except that an insert after a line beginning the way
    /// words follow one another continues it (` More of the sentence.`).
    fn layout(&self) -> Rules {
        Rules::PROSE
    }

    fn collect_errors(&self, _tree: &tree_sitter::Tree, _content: &str) -> Vec<usize> {
        vec![]
    }

    fn build_edits<'language, 'editor>(
        &self,
        editor: &'editor Editor<'language>,
    ) -> Result<Vec<Edit<'editor, 'language>>, String> {
        let mut edits = EditIterator::new(editor).find_edits()?;
        edits.retain(|x| x.nodes().is_none());
        Ok(edits)
    }
}
