use crate::{
    editor::{Edit, EditIterator, Editor},
    languages::{
        LanguageCommon, LanguageEditor, LanguageName,
        ecma_editor::{EcmaEditor, with_comma_variants},
    },
};
use anyhow::Result;
use std::path::Path;

pub fn language() -> LanguageCommon {
    LanguageCommon {
        name: LanguageName::Json,
        file_extensions: &["json"],
        language: tree_sitter_json::LANGUAGE.into(),
        validation_query: None,
        editor: Box::new(JsonEditor::new()),
    }
}

pub struct JsonEditor;

impl Default for JsonEditor {
    fn default() -> Self {
        Self::new()
    }
}

impl JsonEditor {
    pub fn new() -> Self {
        Self
    }
}

impl LanguageEditor for JsonEditor {
    fn format_code(&self, source: &str, file_path: &Path) -> Result<String> {
        EcmaEditor.format_code(source, file_path)
    }

    fn build_edits<'language, 'editor>(
        &self,
        editor: &'editor Editor<'language>,
    ) -> Result<Vec<Edit<'editor, 'language>>, String> {
        Ok(with_comma_variants(EditIterator::new(editor).find_edits()?))
    }
}
