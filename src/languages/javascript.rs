use super::{traits::DefaultEditor, LanguageCommon, LanguageName};
use anyhow::Result;

pub fn language() -> Result<LanguageCommon> {
    let language = tree_sitter_javascript::LANGUAGE.into();
    let editor = Box::new(DefaultEditor::new());

    Ok(LanguageCommon {
        name: LanguageName::Javascript,
        file_extensions: &["js", "jsx"],
        language,
        editor,
        validation_query: None,
    })
}
