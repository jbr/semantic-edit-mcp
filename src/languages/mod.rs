mod ecma_editor;
pub mod javascript;
pub mod json;
pub mod jsx;
pub mod plain;
pub mod python;
pub mod rust;
pub mod toml;
pub mod traits;
pub mod tsx;
pub mod typescript;

use anyhow::Result;
use clap::ValueEnum;
use enum_map::{enum_map, Enum, EnumMap};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    convert::Infallible,
    fmt::{self, Debug, Display, Formatter},
    path::Path,
    result,
    str::FromStr,
};
use strum::IntoStaticStr;
use tree_sitter::{Language, Parser, Query};

use crate::languages::traits::LanguageEditor;

/// Registry to manage all supported languages
#[derive(Debug)]
pub struct LanguageRegistry {
    languages: EnumMap<LanguageName, LanguageCommon>,
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

impl Debug for LanguageCommon {
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
    IntoStaticStr,
    Enum,
    ValueEnum,
)]
#[serde(rename_all = "snake_case")]
#[strum(serialize_all = "snake_case")]
#[repr(u8)]
pub enum LanguageName {
    Rust,
    Json,
    Toml,
    Javascript,
    Typescript,
    Tsx,
    Python,
    Jsx,
    #[serde(other)]
    Other,
}

impl FromStr for LanguageName {
    type Err = Infallible;

    fn from_str(s: &str) -> result::Result<Self, Self::Err> {
        Ok(match s {
            "rust" | "rs" => LanguageName::Rust,
            "json" => LanguageName::Json,
            "toml" => LanguageName::Toml,
            "javascript" | "js" => LanguageName::Javascript,
            "jsx" => LanguageName::Jsx,
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
        let languages = enum_map! {
            LanguageName::Rust => rust::language(),
            LanguageName::Json => json::language(),
            LanguageName::Toml => toml::language(),
            LanguageName::Javascript => javascript::language(),
            LanguageName::Typescript => typescript::language(),
            LanguageName::Tsx => tsx::language(),
            LanguageName::Python => python::language(),
            LanguageName::Jsx => jsx::language(),
            LanguageName::Other => plain::language(),
        };

        let extensions = languages
            .iter()
            .flat_map(|(name, lang)| lang.file_extensions.iter().map(move |ext| (*ext, name)))
            .collect();

        Ok(Self {
            languages,
            extensions,
        })
    }

    pub fn get_language(&self, name: LanguageName) -> &LanguageCommon {
        &self.languages[name]
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
