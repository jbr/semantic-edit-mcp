use super::{traits::LanguageEditor, LanguageCommon, LanguageName};
use anyhow::Result;
use jsonformat::Indentation;
use serde_json::Value;
use std::collections::BTreeMap;
use tree_sitter::Tree;

pub fn language() -> Result<LanguageCommon> {
    let language = tree_sitter_json::LANGUAGE.into();
    let editor = Box::new(JsonEditor::new());
    Ok(LanguageCommon {
        name: LanguageName::Json,
        file_extensions: &["json"],
        language,
        validation_query: None,
        editor,
    })
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
    fn format_code(&self, source: &str) -> Result<String> {
        let mut tab_count = 0;
        let mut space_counts = BTreeMap::<usize, usize>::new();
        let mut last_indentation = 0;
        let mut last_change = 0;
        for line in source.lines().take(100) {
            if line.starts_with('\t') {
                tab_count += 1;
            } else {
                let count = line.chars().take_while(|c| c == &' ').count();
                let diff = count.abs_diff(last_indentation);
                last_indentation = count;
                if diff > 0 {
                    last_change = diff;
                }
                let entry = space_counts.entry(last_change).or_default();
                *entry += 1;
            }
        }

        let custom;

        let indentation_style = match space_counts
            .into_iter()
            .map(|(k, v)| (Some(k), v))
            .chain(std::iter::once((None, tab_count)))
            .max_by_key(|(_, count)| *count)
        {
            Some((Some(2), _)) => Indentation::TwoSpace,
            Some((Some(4), _)) => Indentation::FourSpace,
            Some((None, _)) => Indentation::Tab,
            Some((Some(n), _)) => {
                custom = " ".repeat(n);
                Indentation::Custom(&custom)
            }
            None => Indentation::FourSpace,
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
}
