use crate::languages::{LanguageCommon, LanguageName, traits::LanguageEditor};
use anyhow::Result;
use std::{ops::Range, path::Path};
use taplo::rowan::{TextRange, TextSize};
use tree_sitter::Tree;

pub fn language() -> LanguageCommon {
    LanguageCommon {
        name: LanguageName::Toml,
        file_extensions: &["toml"],
        language: tree_sitter_toml_ng::LANGUAGE.into(),
        editor: Box::new(TomlEditor::new()),
        validation_query: None,
    }
}

pub struct TomlEditor;

impl Default for TomlEditor {
    fn default() -> Self {
        Self::new()
    }
}

impl TomlEditor {
    pub fn new() -> Self {
        Self
    }
}

impl LanguageEditor for TomlEditor {
    fn format_code(&self, source: &str, _file_path: &Path) -> Result<String> {
        Ok(taplo::formatter::format(
            source,
            taplo::formatter::Options::default(),
        ))
    }

    fn collect_errors(&self, _tree: &Tree, content: &str) -> Vec<usize> {
        let converter = LineConverter::new(content);

        taplo::parser::parse(content)
            .errors
            .into_iter()
            .flat_map(|error| converter.range_to_lines(error.range))
            .collect()
    }
}

struct LineConverter {
    newline_positions: Vec<usize>,
}

impl LineConverter {
    fn new(text: &str) -> Self {
        let newline_positions = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(i, _)| i + 1))
            .chain(std::iter::once(text.len())) // End of file
            .collect();

        Self { newline_positions }
    }

    fn textsize_to_line(&self, offset: TextSize) -> usize {
        let byte_offset = usize::from(offset); // Safe conversion
        match self.newline_positions.binary_search(&byte_offset) {
            Ok(line) => line + 1,
            Err(line) => line,
        }
    }

    fn range_to_lines(&self, range: TextRange) -> Range<usize> {
        Range {
            start: self.textsize_to_line(range.start()),
            end: self.textsize_to_line(range.end()),
        }
    }
}
