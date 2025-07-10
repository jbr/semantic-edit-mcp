use crate::languages::{ecma_editor::EcmaEditor, LanguageCommon, LanguageName};

pub fn language() -> LanguageCommon {
    LanguageCommon {
        name: LanguageName::Javascript,
        file_extensions: &["jsx"],
        language: tree_sitter_javascript::LANGUAGE.into(),
        editor: Box::new(EcmaEditor),
        validation_query: None,
    }
}
