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
pub use semantic_grouping::{SemanticGrouping, WithSemanticGrouping, GroupingRule, SemanticGroup};

use anyhow::Result;
use std::collections::HashMap;

/// Registry to manage all supported languages
pub struct LanguageRegistry {
    languages: HashMap<String, Box<dyn LanguageSupport>>,
}

impl LanguageRegistry {
    pub fn new() -> Result<Self> {
        let mut languages: HashMap<String, Box<dyn LanguageSupport>> = HashMap::new();

        // Register JSON language
        languages.insert("json".to_string(), Box::new(json::JsonLanguage::new()?));

        // Register Markdown language
        languages.insert(
            "markdown".to_string(),
            Box::new(markdown::MarkdownLanguage::new()?),
        );

        // Register Rust language
        languages.insert("rust".to_string(), Box::new(rust::RustLanguage::new()?));

        // TODO: Register other languages here as we implement them
        // languages.insert("toml".to_string(), Box::new(TomlLanguage::new()?));

        Ok(Self { languages })
    }

    pub fn get_language(&self, name: &str) -> Option<&dyn LanguageSupport> {
        self.languages.get(name).map(|l| l.as_ref())
    }

    pub fn detect_language_from_path(&self, file_path: &str) -> Option<String> {
        if let Some(extension) = std::path::Path::new(file_path).extension() {
            match extension.to_str()? {
                "rs" => Some("rust".to_string()),
                "json" => Some("json".to_string()),
                "toml" => Some("toml".to_string()),
                "md" | "markdown" => Some("markdown".to_string()),
                "ts" | "tsx" => Some("typescript".to_string()),
                "js" | "jsx" => Some("javascript".to_string()),
                "py" => Some("python".to_string()),
                _ => None,
            }
        } else {
            None
        }
    }

    pub fn supported_languages(&self) -> Vec<&String> {
        self.languages.keys().collect()
    }
}
