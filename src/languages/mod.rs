pub mod javascript;
pub mod json;
pub mod markdown;
pub mod python;
pub mod rust;
pub mod toml;
pub mod traits;
pub mod tsx;
pub mod typescript;
pub mod utils;

use anyhow::{anyhow, Result};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, fmt::Display, path::Path};
use tree_sitter::{Language, Parser, Query};

use crate::languages::traits::{LanguageEditor, NodeTypeInfo};

/// Registry to manage all supported languages
#[derive(Debug)]
pub struct LanguageRegistry {
    languages: HashMap<LanguageName, LanguageCommon>,
    extensions: HashMap<&'static str, LanguageName>,
}

#[derive(fieldwork::Fieldwork)]
#[fieldwork(get)]
pub struct LanguageCommon {
    #[fieldwork(get(copy))]
    name: LanguageName,
    file_extensions: &'static [&'static str],
    #[fieldwork(rename = tree_sitter_language)]
    language: Language,
    node_types: Vec<NodeTypeInfo>,
    editor: Box<dyn LanguageEditor>,
    validation_query: Option<Query>,
}

impl std::fmt::Debug for LanguageCommon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LanguageCommon")
            .field("name", &self.name)
            .field("file_extensions", &self.file_extensions)
            .field("language", &self.language)
            .field("node_types", &self.node_types)
            .field("validation_query", &self.validation_query)
            .finish()
    }
}
impl Display for LanguageCommon {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name.as_str())
    }
}

impl LanguageCommon {
    pub fn tree_sitter_parser(&self) -> Result<Parser> {
        let mut parser = Parser::new();
        parser.set_language(self.tree_sitter_language())?;
        Ok(parser)
    }

    pub fn docs(&self) -> String {
        let node_types = self.node_types();
        let name = self.name();
        let named_types = node_types
            .iter()
            .filter(|nt| nt.named)
            .map(|nt| &*nt.node_type)
            .collect::<Vec<_>>()
            .join(", ");
        format!("Node types you can use for {name}: {named_types}")
    }
}

#[derive(
    Serialize, Deserialize, Debug, JsonSchema, Hash, Eq, PartialEq, Ord, PartialOrd, Clone, Copy,
)]
#[serde(rename_all = "snake_case")]
pub enum LanguageName {
    Rust,
    Json,
    Markdown,
    Toml,
    Javascript,
    Typescript,
    Tsx,
    Python,
}
impl LanguageName {
    fn as_str(&self) -> &str {
        match self {
            LanguageName::Rust => "rust",
            LanguageName::Json => "json",
            LanguageName::Markdown => "markdown",
            LanguageName::Toml => "toml",
            LanguageName::Javascript => "javascript",
            LanguageName::Typescript => "typescript",
            LanguageName::Tsx => "tsx",
            LanguageName::Python => "python",
        }
    }
}

impl Display for LanguageName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
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
        registry.register_language(toml::language()?);
        registry.register_language(typescript::language()?);
        registry.register_language(tsx::language()?);
        registry.register_language(javascript::language()?);
        registry.register_language(python::language()?);

        Ok(registry)
    }

    pub fn register_language(&mut self, language: LanguageCommon) {
        let name = language.name();
        for extension in language.file_extensions() {
            self.extensions.insert(extension, name);
        }
        self.languages.insert(name, language);
    }

    pub fn get_language(&self, name: LanguageName) -> Option<&LanguageCommon> {
        self.languages.get(&name)
    }

    pub fn get_language_with_hint(
        &self,
        file_path: &Path,
        language_hint: Option<LanguageName>,
    ) -> Result<&LanguageCommon> {
        let language_name = language_hint
            .or_else(|| self.detect_language_from_path(file_path))
            .ok_or_else(|| {
                anyhow!("Unable to detect language from file path and no language hint provided")
            })?;
        self.get_language(language_name)
            .ok_or_else(|| anyhow!("Unsupported language {language_name}"))
    }

    pub fn detect_language_from_path(&self, file_path: &Path) -> Option<LanguageName> {
        let extension = file_path.extension()?.to_str()?;
        self.extensions.get(extension).copied()
    }
}
