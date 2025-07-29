use crate::languages::{LanguageCommon, LanguageName, ecma_editor::EcmaEditor};

pub fn language() -> LanguageCommon {
    LanguageCommon {
        name: LanguageName::Tsx,
        file_extensions: &["tsx"],
        language: tree_sitter_typescript::LANGUAGE_TSX.into(),
        editor: Box::new(EcmaEditor),
        validation_query: None,
    }
}
