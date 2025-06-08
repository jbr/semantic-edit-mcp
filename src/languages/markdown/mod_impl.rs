use crate::languages::traits::{
    LanguageEditor, LanguageParser, LanguageQueries, LanguageSupport, NodeTypeInfo,
};
use crate::languages::utils::{load_query_file, parse_node_types_json};
use crate::languages::QueryBasedParser;
use anyhow::Result;
use tree_sitter::Language;

pub struct MarkdownLanguage;

impl MarkdownLanguage {
    pub fn new() -> Result<Self> {
        Ok(Self)
    }
}

impl LanguageSupport for MarkdownLanguage {
    fn language_name(&self) -> &'static str {
        "markdown"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["md", "markdown"]
    }

    fn tree_sitter_language(&self) -> Language {
        tree_sitter_md::LANGUAGE.into()
    }

    fn get_node_types(&self) -> Result<Vec<NodeTypeInfo>> {
        let node_types_json = include_str!("../../../queries/markdown/node-types.json");
        parse_node_types_json(node_types_json)
    }

    fn load_queries(&self) -> Result<LanguageQueries> {
        let language = self.tree_sitter_language();
        let mut queries = LanguageQueries::new();

        // Load operations query
        queries.operations = load_query_file(&language, "queries/markdown/operations.scm")?;

        // TODO: Load other standard query files as needed
        // queries.highlights = load_query_file(&language, "queries/markdown/highlights.scm")?;

        Ok(queries)
    }

    fn parser(&self) -> Box<dyn LanguageParser> {
        let language = self.tree_sitter_language();
        let queries = self
            .load_queries()
            .unwrap_or_else(|_| LanguageQueries::new());
        let node_types = self.get_node_types().unwrap_or_default();

        Box::new(QueryBasedParser::new(language, queries, node_types))
    }

    fn editor(&self) -> Box<dyn LanguageEditor> {
        Box::new(super::editor::MarkdownEditor::new())
    }
}
