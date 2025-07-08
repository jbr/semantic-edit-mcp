pub mod javascript;
pub mod json;
pub mod plain;
pub mod python;
pub mod rust;
pub mod toml;
pub mod traits;
pub mod tsx;
pub mod typescript;

use anyhow::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    convert::Infallible,
    fmt::{self, Display, Formatter},
    path::Path,
    str::FromStr,
};
use tree_sitter::{Language, Parser, Query};

use crate::languages::traits::LanguageEditor;

/// Registry to manage all supported languages
#[derive(Debug)]
pub struct LanguageRegistry {
    languages: HashMap<LanguageName, LanguageCommon>,
    extensions: HashMap<&'static str, LanguageName>,
}

#[derive(fieldwork::Fieldwork)]
#[fieldwork(get)]
pub struct LanguageCommon {
    #[field(copy)]
    name: LanguageName,
    file_extensions: &'static [&'static str],
    #[field = "tree_sitter_language"]
    language: Language,
    editor: Box<dyn LanguageEditor>,
    validation_query: Option<Query>,
}

impl fmt::Debug for LanguageCommon {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("LanguageCommon")
            .field("name", &self.name)
            .field("file_extensions", &self.file_extensions)
            .field("language", &self.language)
            .field("validation_query", &self.validation_query)
            .finish()
    }
}
impl Display for LanguageCommon {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(self.name.as_str())
    }
}

impl LanguageCommon {
    pub fn tree_sitter_parser(&self) -> Result<Parser> {
        let mut parser = Parser::new();
        parser.set_language(self.tree_sitter_language())?;
        Ok(parser)
    }
}

#[derive(
    Serialize,
    Deserialize,
    Debug,
    JsonSchema,
    Hash,
    Eq,
    PartialEq,
    Ord,
    PartialOrd,
    Clone,
    Copy,
    strum::VariantNames,
    strum::IntoStaticStr,
    clap::ValueEnum,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
pub enum LanguageName {
    Rust,
    Json,
    Toml,
    Javascript,
    Typescript,
    Tsx,
    Python,
    #[serde(other)]
    Other,
}

impl FromStr for LanguageName {
    type Err = Infallible;

    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        Ok(match s {
            "rust" | "rs" => LanguageName::Rust,
            "json" => LanguageName::Json,
            "toml" => LanguageName::Toml,
            "javascript" | "js" | "jsx" => LanguageName::Javascript,
            "ts" | "typescript" => LanguageName::Typescript,
            "tsx" => LanguageName::Tsx,
            "py" | "python" => LanguageName::Python,
            _ => LanguageName::Other,
        })
    }
}

impl LanguageName {
    fn as_str(&self) -> &'static str {
        self.into()
    }
}

impl Display for LanguageName {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
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
        registry.register_language(rust::language()?);
        registry.register_language(toml::language()?);
        registry.register_language(typescript::language()?);
        registry.register_language(tsx::language()?);
        registry.register_language(javascript::language()?);
        registry.register_language(python::language()?);
        registry.register_language(plain::language()?);

        Ok(registry)
    }

    pub fn register_language(&mut self, language: LanguageCommon) {
        let name = language.name();
        for extension in language.file_extensions() {
            self.extensions.insert(extension, name);
        }
        self.languages.insert(name, language);
    }

    pub fn get_language(&self, name: LanguageName) -> &LanguageCommon {
        self.languages.get(&name).unwrap()
    }

    pub fn get_language_with_hint(
        &self,
        file_path: &Path,
        language_hint: Option<LanguageName>,
    ) -> Result<&LanguageCommon> {
        let language_name = language_hint
            .or_else(|| self.detect_language_from_path(file_path))
            .unwrap_or(LanguageName::Other);
        Ok(self.get_language(language_name))
    }

    pub fn detect_language_from_path(&self, file_path: &Path) -> Option<LanguageName> {
        let extension = file_path.extension()?.to_str()?;
        self.extensions.get(extension).copied()
    }
}
