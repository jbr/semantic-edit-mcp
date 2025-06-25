mod editor;
use crate::languages::LanguageName;

use super::{utils::parse_node_types_json, LanguageCommon};
use anyhow::Result;
use editor::JsonEditor;

pub fn language() -> Result<LanguageCommon> {
    let language = tree_sitter_json::LANGUAGE.into();
    let node_types = parse_node_types_json(tree_sitter_json::NODE_TYPES)?;
    let editor = Box::new(JsonEditor::new());
    Ok(LanguageCommon {
        name: LanguageName::Json,
        file_extensions: &["json"],
        language,
        validation_query: None,
        node_types,
        editor,
    })
}
