use crate::languages::traits::{LanguageSupport, LanguageParser, LanguageEditor, NodeTypeInfo, LanguageQueries};
use crate::languages::utils::{parse_node_types_json, load_query_file};
use crate::languages::QueryBasedParser;
use anyhow::Result;
use tree_sitter::Language;

pub struct JsonLanguage;

impl JsonLanguage {
    pub fn new() -> Result<Self> {
        Ok(Self)
    }
}

impl LanguageSupport for JsonLanguage {
    fn language_name(&self) -> &'static str {
        "json"
    }
    
    fn file_extensions(&self) -> &'static [&'static str] {
        &["json"]
    }
    
    fn tree_sitter_language(&self) -> Language {
        tree_sitter_json::LANGUAGE.into()
    }
    
    fn get_node_types(&self) -> Result<Vec<NodeTypeInfo>> {
        let node_types_json = include_str!("../../../queries/json/node-types.json");
        parse_node_types_json(node_types_json)
    }
    
    fn load_queries(&self) -> Result<LanguageQueries> {
        let language = self.tree_sitter_language();
        let mut queries = LanguageQueries::new();
        
        // Load operations query
        queries.operations = load_query_file(&language, "queries/json/operations.scm")?;
        
        // TODO: Load other standard query files as needed
        // queries.highlights = load_query_file(&language, "queries/json/highlights.scm")?;
        
        Ok(queries)
    }
    
    fn parser(&self) -> Box<dyn LanguageParser> {
        let language = self.tree_sitter_language();
        let queries = self.load_queries().unwrap_or_else(|_| LanguageQueries::new());
        let node_types = self.get_node_types().unwrap_or_default();
        
        Box::new(QueryBasedParser::new(language, queries, node_types))
    }
    
    fn editor(&self) -> Box<dyn LanguageEditor> {
        Box::new(super::editor::JsonEditor::new())
    }
}
