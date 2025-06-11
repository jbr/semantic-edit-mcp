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

use anyhow::Result;
use std::{collections::HashMap, path::Path};

use crate::languages::{json::JsonLanguage, markdown::MarkdownLanguage, rust::RustLanguage};

/// Registry to manage all supported languages
pub struct LanguageRegistry {
    languages: HashMap<&'static str, Box<dyn LanguageSupport>>,
}

impl LanguageRegistry {
    pub fn new() -> Result<Self> {
        let mut registry = Self {
            languages: HashMap::new(),
        };

        // Register JSON language
        registry
            .languages
            .insert("json", Box::new(JsonLanguage::new()?));

        // Register Markdown language
        registry
            .languages
            .insert("markdown", Box::new(MarkdownLanguage::new()?));

        // Register Rust language
        registry
            .languages
            .insert("rust", Box::new(RustLanguage::new()?));

        // TODO: Register other languages here as we implement them
        // registry.languages.insert("toml", Box::new(TomlLanguage::new()?));

        Ok(registry)
    }

    pub fn get_language(&self, name: &str) -> Option<&dyn LanguageSupport> {
        self.languages.get(name).map(|l| l.as_ref())
    }

    pub fn detect_language_from_path(&self, file_path: &str) -> Option<&'static str> {
        if let Some(extension) = Path::new(file_path).extension() {
            match extension.to_str()? {
                "rs" => Some("rust"),
                "json" => Some("json"),
                "toml" => Some("toml"),
                "md" | "markdown" => Some("markdown"),
                "ts" | "tsx" => Some("typescript"),
                "js" | "jsx" => Some("javascript"),
                "py" => Some("python"),
                _ => None,
            }
        } else {
            None
        }
    }

    pub fn supported_languages(&self) -> Vec<&'static str> {
        self.languages.keys().map(|x| *x).collect()
    }
}
