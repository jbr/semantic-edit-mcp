use super::{utils::parse_node_types_json, LanguageCommon};
use crate::languages::traits::LanguageEditor;
use anyhow::Result;

pub fn language() -> Result<LanguageCommon> {
    let language = tree_sitter_javascript::LANGUAGE.into();
    let node_types = parse_node_types_json(tree_sitter_javascript::NODE_TYPES)?;
    let editor = Box::new(JavascriptEditor::new());

    Ok(LanguageCommon {
        name: "javascript",
        file_extensions: &["js", "jsx"],
        language,
        node_types,
        editor,
        validation_query: None,
    })
}

pub struct JavascriptEditor;

impl Default for JavascriptEditor {
    fn default() -> Self {
        Self::new()
    }
}

impl JavascriptEditor {
    pub fn new() -> Self {
        Self
    }
}

impl LanguageEditor for JavascriptEditor {}
