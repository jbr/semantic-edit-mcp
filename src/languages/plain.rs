use crate::languages::{traits::DefaultEditor, LanguageCommon, LanguageName};

pub fn language() -> LanguageCommon {
    LanguageCommon {
        name: LanguageName::Other,
        file_extensions: &[],
        language: tree_sitter_plain::LANGUAGE.into(),
        editor: Box::new(DefaultEditor::new()),
        validation_query: None,
    }
}
