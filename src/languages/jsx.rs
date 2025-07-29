use crate::languages::{LanguageCommon, LanguageName, ecma_editor::EcmaEditor};

pub fn language() -> LanguageCommon {
    LanguageCommon {
        name: LanguageName::Javascript,
        file_extensions: &["jsx"],
        language: tree_sitter_javascript::LANGUAGE.into(),
        editor: Box::new(EcmaEditor),
        validation_query: None,
    }
}
