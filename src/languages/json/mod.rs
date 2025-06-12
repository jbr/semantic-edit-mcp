mod editor;
use super::{utils::parse_node_types_json, LanguageCommon};
use anyhow::Result;
use editor::JsonEditor;

pub fn language() -> Result<LanguageCommon> {
    let language = tree_sitter_json::LANGUAGE.into();
    let node_types = parse_node_types_json(tree_sitter_json::NODE_TYPES)?;
    // let validation_query = Some(tree_sitter::Query::new(
    //     &language,
    //     include_str!("../../../queries/json/validation.scm"),
    // )?);
    let editor = Box::new(JsonEditor::new());
    Ok(LanguageCommon {
        name: "json",
        file_extensions: &["json"],
        language,
        validation_query: None,
        node_types,
        editor,
    })
}
