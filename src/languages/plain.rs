use crate::languages::{traits::DefaultEditor, LanguageName};

use super::{utils::parse_node_types_json, LanguageCommon};
use anyhow::Result;

pub fn language() -> Result<LanguageCommon> {
    let language = tree_sitter_plain::LANGUAGE.into();
    let node_types = parse_node_types_json(tree_sitter_plain::NODE_TYPES)?;
    let editor = Box::new(DefaultEditor::new());

    Ok(LanguageCommon {
        name: LanguageName::Other,
        file_extensions: &[],
        language,
        node_types,
        editor,
        validation_query: None,
    })
}
