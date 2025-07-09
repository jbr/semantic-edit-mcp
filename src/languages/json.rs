use std::path::Path;

use crate::editor::{Edit, EditIterator, Editor};

use super::{common::Indentation, traits::LanguageEditor, LanguageCommon, LanguageName};
use anyhow::Result;
use jsonformat::Indentation as JsonIndentation;
use serde_json::Value;
use tree_sitter::Tree;

pub fn language() -> LanguageCommon {
    LanguageCommon {
        name: LanguageName::Json,
        file_extensions: &["json"],
        language: tree_sitter_json::LANGUAGE.into(),
        validation_query: None,
        editor: Box::new(JsonEditor::new()),
    }
}

pub struct JsonEditor;

impl Default for JsonEditor {
    fn default() -> Self {
        Self::new()
    }
}

impl JsonEditor {
    pub fn new() -> Self {
        Self
    }
}

impl LanguageEditor for JsonEditor {
    fn format_code(&self, source: &str, _file_path: &Path) -> Result<String> {
        let custom;

        let indentation_style = match Indentation::determine(source) {
            Some(Indentation::Spaces(2)) => JsonIndentation::TwoSpace,
            Some(Indentation::Spaces(4)) => JsonIndentation::FourSpace,
            Some(Indentation::Tabs) => JsonIndentation::Tab,
            Some(Indentation::Spaces(n)) => {
                custom = " ".repeat(n.into());
                JsonIndentation::Custom(&custom)
            }
            None => JsonIndentation::FourSpace,
        };

        Ok(jsonformat::format(source, indentation_style))
    }

    fn collect_errors(&self, _tree: &Tree, content: &str) -> Vec<usize> {
        match serde_json::from_str::<Value>(content) {
            Ok(_) => vec![],
            Err(e) => {
                vec![e.line().saturating_sub(1)]
            }
        }
    }

    fn build_edits<'language, 'editor>(
        &self,
        editor: &'editor Editor<'language>,
    ) -> Result<Vec<Edit<'editor, 'language>>, String> {
        EditIterator::new(editor).find_edits()
    }
}
