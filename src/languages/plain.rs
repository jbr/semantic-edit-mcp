use crate::{
    editor::{Edit, EditIterator, Editor},
    languages::{LanguageCommon, LanguageName, traits::LanguageEditor},
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
    fn collect_errors(&self, _tree: &tree_sitter::Tree, _content: &str) -> Vec<usize> {
        vec![]
    }

    fn build_edits<'language, 'editor>(
        &self,
        editor: &'editor Editor<'language>,
    ) -> Result<Vec<Edit<'editor, 'language>>, String> {
        let mut edits = EditIterator::new(editor).find_edits()?;
        edits.retain(|x| x.nodes().is_none());
        // A whitespace-insensitive anchor begins at its first non-whitespace
        // byte, leaving the line's indentation outside the replacement range.
        // If content supplies indentation explicitly, replace that prefix too;
        // otherwise preserve it. Continuation lines are never reindented.
        if editor.selector().operation == crate::selector::Operation::Replace {
            for edit in &mut edits {
                if !edit.content().starts_with([' ', '\t']) {
                    continue;
                }
                let start = edit.start_byte();
                let source = edit.source_code();
                let line_start = source[..start].rfind('\n').map_or(0, |index| index + 1);
                if source[line_start..start]
                    .chars()
                    .all(|c| matches!(c, ' ' | '\t'))
                {
                    edit.set_start_byte(line_start);
                }
            }
        }

        Ok(edits)
    }
}
