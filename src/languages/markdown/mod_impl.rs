use crate::languages::traits::{LanguageEditor, LanguageQueries, LanguageSupport, NodeTypeInfo};
use crate::languages::utils::{load_query_file, parse_node_types_json};
use crate::languages::LanguageCommon;
use anyhow::Result;
use tree_sitter::Language;

pub struct MarkdownLanguage(LanguageCommon);

impl MarkdownLanguage {
    pub fn new() -> Result<Self> {
        let language = tree_sitter_md::LANGUAGE.into();

        let mut queries = LanguageQueries::new();

        queries.operations = load_query_file(&language, "queries/markdown/operations.scm")?;

        let node_types = parse_node_types_json(tree_sitter_md::NODE_TYPES_BLOCK)?;

        Ok(Self(LanguageCommon {
            language,
            queries,
            node_types,
        }))
    }
}

impl LanguageSupport for MarkdownLanguage {
    fn language_name(&self) -> &'static str {
        "markdown"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["md", "markdown"]
    }

    fn tree_sitter_language(&self) -> &Language {
        &self.0.language
    }

    fn node_types(&self) -> &[NodeTypeInfo] {
        &self.0.node_types
    }

    fn queries(&self) -> &LanguageQueries {
        &self.0.queries
    }

    fn editor(&self) -> Box<dyn LanguageEditor> {
        Box::new(super::editor::MarkdownEditor::new())
    }
}
