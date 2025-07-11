use crate::{
    editor::{Edit, EditIterator, Editor},
    languages::{ecma_editor::EcmaEditor, LanguageCommon, LanguageEditor, LanguageName},
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
        let mut edits = EditIterator::new(editor).find_edits()?;

        let new_edits = edits
            .iter()
            .filter(|edit| !edit.content().ends_with(','))
            .cloned()
            .map(Edit::modify(|edit| {
                edit.set_annotation("added trailing comma")
                    .content_mut()
                    .to_mut()
                    .push(',')
            }))
            .collect::<Vec<_>>();

        edits.extend(new_edits);
        Ok(edits)
    }
}
