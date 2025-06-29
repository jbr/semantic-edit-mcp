use crate::languages::{traits::LanguageEditor, LanguageCommon, LanguageName};
use anyhow::Result;

pub fn language() -> Result<LanguageCommon> {
    let language = tree_sitter_typescript::LANGUAGE_TSX.into();
    let editor = Box::new(TypescriptEditor::new());

    Ok(LanguageCommon {
        name: LanguageName::Tsx,
        file_extensions: &["tsx"],
        language,
        editor,
        validation_query: None,
    })
}

pub struct TypescriptEditor;

impl Default for TypescriptEditor {
    fn default() -> Self {
        Self::new()
    }
}

impl TypescriptEditor {
    pub fn new() -> Self {
        Self
    }
}

impl LanguageEditor for TypescriptEditor {}
