use crate::languages::{ecma_editor::EcmaEditor, LanguageCommon, LanguageName};

pub fn language() -> LanguageCommon {
    LanguageCommon {
        name: LanguageName::Tsx,
        file_extensions: &["tsx"],
        language: tree_sitter_typescript::LANGUAGE_TSX.into(),
        editor: Box::new(EcmaEditor),
        validation_query: None,
    }
}
