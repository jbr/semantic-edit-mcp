use super::{LanguageCommon, LanguageName, ecma_editor::EcmaEditor};

pub fn language() -> LanguageCommon {
    LanguageCommon {
        name: LanguageName::Javascript,
        file_extensions: &["js"],
        language: tree_sitter_javascript::LANGUAGE.into(),
        editor: Box::new(EcmaEditor),
        validation_query: None,
    }
}
