pub mod json;
pub mod markdown;
pub mod query_parser;
pub mod rust;
pub mod semantic_grouping;
pub mod traits;
pub mod utils;

#[cfg(test)]
mod semantic_grouping_tests;

// Re-export key types for easier access
pub use query_parser::QueryBasedParser;
pub use traits::LanguageSupport;

use anyhow::{anyhow, Result};
use std::{collections::HashMap, path::Path};
use tree_sitter::Language;

use crate::languages::{
    json::JsonLanguage,
    markdown::MarkdownLanguage,
    rust::RustLanguage,
    traits::{LanguageQueries, NodeTypeInfo},
};

/// Registry to manage all supported languages
pub struct LanguageRegistry {
    languages: HashMap<&'static str, Box<dyn LanguageSupport>>,
    extensions: HashMap<&'static str, &'static str>,
}

struct LanguageCommon {
    language: Language,
    queries: LanguageQueries,
    node_types: Vec<NodeTypeInfo>,
}

impl LanguageRegistry {
    pub fn new() -> Result<Self> {
        let mut registry = Self {
            languages: HashMap::new(),
            extensions: HashMap::new(),
        };

        // Register JSON language
        registry.register_language(JsonLanguage::new()?);

        // Register Markdown language
        registry.register_language(MarkdownLanguage::new()?);

        // Register Rust language
        registry.register_language(RustLanguage::new()?);

        // TODO: Register other languages here as we implement them
        // registry.languages.insert("toml", Box::new(TomlLanguage::new()?));

        Ok(registry)
    }

    pub fn register_language(&mut self, language: impl LanguageSupport + 'static) {
        let name = language.language_name();
        for extension in language.file_extensions() {
            self.extensions.insert(extension, name);
        }
        self.languages.insert(name, Box::new(language));
    }

    pub fn get_language(&self, name: &str) -> Option<&dyn LanguageSupport> {
        self.languages.get(name).map(|l| l.as_ref())
    }

    pub fn get_language_with_hint(
        &self,
        file_path: &str,
        language_hint: Option<&str>,
    ) -> Result<&dyn LanguageSupport> {
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
}
