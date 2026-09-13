use crate::{
    editor::{Edit, EditIterator, Editor},
    languages::{LanguageCommon, LanguageName, traits::LanguageEditor},
};
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

    // A tree-sitter-toml `table` node encompasses *all* of its key/value pairs, so
    // an anchor that straddles a table boundary makes the generic node-snapping
    // candidates (`siblings_in_range` / common-parent) expand the *replaced* span
    // across whole sibling tables. Applied, that silently deletes the `[table]`
    // headers and bodies the anchor never touched — and because the result is still
    // valid TOML, nothing rejects it, so the wider (earlier) candidate wins over the
    // surgical one.
    //
    // The candidates differ only in how many bytes they delete (the inserted
    // content is identical), so we prefer the *least destructive* one: a stable sort
    // by deleted-byte count. For a `replace` this floats the exact text-anchored
    // candidate to the front while leaving the node-snapping candidates as
    // fallbacks (e.g. for replacing a whole `pair`, where the exact span alone won't
    // re-parse). Inserts delete nothing (`end_byte` is `None` → 0), so they all tie
    // and keep their original order — `insert_after` a `[table]` still snaps to the
    // end of the whole table. A candidate that produces malformed TOML is still
    // rejected by the parse-error gate (`collect_errors`), and taplo reformats the
    // winner afterward.
    fn build_edits<'language, 'editor>(
        &self,
        editor: &'editor Editor<'language>,
    ) -> Result<Vec<Edit<'editor, 'language>>, String> {
        let mut edits = EditIterator::new(editor).find_edits()?;
        edits.sort_by_key(|edit| {
            let position = edit.position();
            position.end_byte.unwrap_or(position.start_byte) - position.start_byte
        });
        Ok(edits)
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
    /// Byte offset at which each line begins; `line_starts[0]` is always 0.
    line_starts: Vec<usize>,
}

impl LineConverter {
    fn new(text: &str) -> Self {
        let line_starts = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(i, _)| i + 1))
            .collect();

        Self { line_starts }
    }

    /// Zero-based line index containing `offset`, matching `str::lines().enumerate()`
    /// (the indexing the error display in `Editor::syntax_errors` compares against).
    fn textsize_to_line(&self, offset: TextSize) -> usize {
        let byte_offset = usize::from(offset);
        // Index of the last line whose start is <= offset.
        self.line_starts
            .partition_point(|&start| start <= byte_offset)
            .saturating_sub(1)
    }

    fn range_to_lines(&self, range: TextRange) -> Range<usize> {
        let start = self.textsize_to_line(range.start());
        let end = self.textsize_to_line(range.end());
        // End-inclusive: a single-line error has `start == end`, so a bare
        // `start..end` would be empty and the error would be silently dropped by the
        // `flat_map` in `collect_errors` — which (since nearly every TOML error is
        // single-line) is exactly what let malformed edits pass validation.
        start..end + 1
    }
}
