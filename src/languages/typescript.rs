use super::{utils::parse_node_types_json, LanguageCommon, LanguageName};
use crate::languages::traits::DefaultEditor;
use anyhow::Result;

pub fn language() -> Result<LanguageCommon> {
    let language = tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into();
    let node_types = parse_node_types_json(tree_sitter_typescript::TYPESCRIPT_NODE_TYPES)?;
    let editor = Box::new(DefaultEditor::new());

    Ok(LanguageCommon {
        name: LanguageName::Typescript,
        file_extensions: &["ts"],
        language,
        node_types,
        editor,
        validation_query: None,
    })
}
