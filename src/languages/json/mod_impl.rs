use crate::languages::traits::{LanguageEditor, LanguageQueries, LanguageSupport, NodeTypeInfo};
use crate::languages::utils::{load_query_file, parse_node_types_json};
use crate::languages::LanguageCommon;
use anyhow::Result;
use tree_sitter::Language;

pub struct JsonLanguage(LanguageCommon);

impl JsonLanguage {
    pub fn new() -> Result<Self> {
        let language = tree_sitter_json::LANGUAGE.into();

        let mut queries = LanguageQueries::new();

        queries.operations = load_query_file(&language, "queries/json/operations.scm")?;

        let node_types = parse_node_types_json(tree_sitter_json::NODE_TYPES)?;

        Ok(Self(LanguageCommon {
            language,
            queries,
            node_types,
        }))
    }
}

impl LanguageSupport for JsonLanguage {
    fn language_name(&self) -> &'static str {
        "json"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["json"]
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
        Box::new(super::editor::JsonEditor::new())
    }
}
