use super::{ecma_editor::EcmaEditor, LanguageCommon, LanguageName};

pub fn language() -> LanguageCommon {
    LanguageCommon {
        name: LanguageName::Javascript,
        file_extensions: &["js"],
        language: tree_sitter_javascript::LANGUAGE.into(),
        editor: Box::new(EcmaEditor::Js),
        validation_query: None,
    }
}
