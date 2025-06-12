pub mod editor;
use super::{utils::parse_node_types_json, LanguageCommon};
use anyhow::Result;

pub fn language() -> Result<LanguageCommon> {
    let language = tree_sitter_md::LANGUAGE.into();
    let node_types = parse_node_types_json(tree_sitter_md::NODE_TYPES_BLOCK)?;
    let editor = Box::new(editor::MarkdownEditor::new());

    Ok(LanguageCommon {
        name: "markdown",
        file_extensions: &["md", "markdown"],
        language,
        node_types,
        editor,
        validation_query: None,
    })
}
