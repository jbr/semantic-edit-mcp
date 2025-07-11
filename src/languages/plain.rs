use crate::{
    editor::{Edit, EditIterator, Editor},
    languages::{traits::LanguageEditor, LanguageCommon, LanguageName},
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

    fn format_code(&self, source: &str, file_path: &std::path::Path) -> Result<String> {
        let _ = file_path;
        Ok(source.to_string())
    }

    fn build_edits<'language, 'editor>(
        &self,
        editor: &'editor Editor<'language>,
    ) -> Result<Vec<Edit<'editor, 'language>>, String> {
        let mut edits = EditIterator::new(editor).find_edits()?;
        edits.retain(|x| x.node().is_none());

        Ok(edits)
    }
}
