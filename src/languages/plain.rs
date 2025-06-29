use crate::languages::{traits::DefaultEditor, LanguageCommon, LanguageName};
use anyhow::Result;

pub fn language() -> Result<LanguageCommon> {
    let language = tree_sitter_plain::LANGUAGE.into();
    let editor = Box::new(DefaultEditor::new());

    Ok(LanguageCommon {
        name: LanguageName::Other,
        file_extensions: &[],
        language,
        editor,
        validation_query: None,
    })
}
