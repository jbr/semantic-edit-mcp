use super::{LanguageCommon, LanguageName};
use crate::languages::ecma_editor::EcmaEditor;

pub fn language() -> LanguageCommon {
    LanguageCommon {
        name: LanguageName::Typescript,
        file_extensions: &["ts"],
        language: tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        editor: Box::new(EcmaEditor::Ts),
        validation_query: None,
    }
}
