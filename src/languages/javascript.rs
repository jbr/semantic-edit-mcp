use super::{utils::parse_node_types_json, LanguageCommon, LanguageName};
use crate::languages::traits::DefaultEditor;
use anyhow::Result;

pub fn language() -> Result<LanguageCommon> {
    let language = tree_sitter_javascript::LANGUAGE.into();
    let node_types = parse_node_types_json(tree_sitter_javascript::NODE_TYPES)?;
    let editor = Box::new(DefaultEditor::new());

    Ok(LanguageCommon {
        name: LanguageName::Javascript,
        file_extensions: &["js", "jsx"],
        language,
        node_types,
        editor,
        validation_query: None,
    })
}
