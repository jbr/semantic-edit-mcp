pub mod json;
pub mod markdown;
pub mod rust;
pub mod semantic_grouping;
pub mod traits;
pub mod utils;

#[cfg(test)]
mod semantic_grouping_tests;

use anyhow::{anyhow, Result};
use std::{collections::HashMap, path::Path};
use tree_sitter::{Language, Parser, Query};

use crate::languages::traits::{LanguageEditor, NodeTypeInfo};

/// Registry to manage all supported languages
pub struct LanguageRegistry {
    languages: HashMap<&'static str, LanguageCommon>,
    extensions: HashMap<&'static str, &'static str>,
}

#[derive(fieldwork::Fieldwork)]
#[fieldwork(get)]
pub struct LanguageCommon {
    #[fieldwork(get(copy))]
    name: &'static str,
    #[fieldwork(get(copy))]
    file_extensions: &'static [&'static str],
    #[fieldwork(rename = tree_sitter_language)]
    language: Language,
    node_types: Vec<NodeTypeInfo>,
    #[fieldwork(skip)]
    editor: Box<dyn LanguageEditor>,
    #[fieldwork(skip)]
    validation_query: Option<Query>,
}

impl LanguageCommon {
    pub fn editor(&self) -> &dyn LanguageEditor {
        &*self.editor
    }

    pub fn tree_sitter_parser(&self) -> Result<Parser> {
        let mut parser = Parser::new();
        parser.set_language(self.tree_sitter_language())?;
        Ok(parser)
    }

    pub fn validation_query(&self) -> Option<&Query> {
        self.validation_query.as_ref()
    }
}

impl LanguageRegistry {
    pub fn new() -> Result<Self> {
        let mut registry = Self {
            languages: HashMap::new(),
            extensions: HashMap::new(),
        };

        registry.register_language(json::language()?);
        registry.register_language(markdown::language()?);
        registry.register_language(rust::language()?);

        Ok(registry)
    }

    pub fn register_language(&mut self, language: LanguageCommon) {
        let name = language.name();
        for extension in language.file_extensions() {
            self.extensions.insert(extension, name);
        }
        self.languages.insert(name, language);
    }

    pub fn get_language(&self, name: &str) -> Option<&LanguageCommon> {
        self.languages.get(name)
    }

    pub fn get_language_with_hint(
        &self,
        file_path: &str,
        language_hint: Option<&str>,
    ) -> Result<&LanguageCommon> {
        let language_name = language_hint
            .or_else(|| self.detect_language_from_path(file_path))
            .ok_or_else(|| {
                anyhow!("Unable to detect language from file path and no language hint provided")
            })?;
        self.get_language(language_name)
            .ok_or_else(|| anyhow!("Unsupported language {language_name}"))
    }

    pub fn detect_language_from_path(&self, file_path: &str) -> Option<&'static str> {
        let extension = Path::new(file_path).extension()?.to_str()?;
        self.extensions.get(extension).copied()
    }

    pub(crate) fn get_documentation(&self, language: &str) -> Result<String> {
        let language = self
            .get_language(language)
            .ok_or_else(|| anyhow!("language not recogized"))?;
        let node_types = language.node_types();
        let name = language.name();
        let named_types = node_types
            .iter()
            .filter(|nt| nt.named)
            .map(|nt| &*nt.node_type)
            .collect::<Vec<_>>()
            .join(", ");
        Ok(format!("Node types you can use for {name}: {named_types}"))
    }
}
