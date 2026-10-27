use crate::{
    editor::{Edit, EditIterator, Editor},
    languages::{LanguageCommon, LanguageName, traits::LanguageEditor},
    layout::Rules,
    validation::Diagnostic,
};
use anyhow::Result;
use std::ops::Range;
use taplo::{
    dom::{Error, node::Key},
    rowan::{TextRange, TextSize},
};
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
    /// A line inside a multi-line string (`"""…"""` or `'''…'''`) is never
    /// re-indented.
    fn layout(&self) -> Rules {
        Rules {
            literals: &["string"],
            ..Rules::CODE
        }
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
    // rejected by the parse-error gate (`collect_errors`), and the configured
    // formatter, if any, reformats the winner afterward.
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

    /// taplo's DOM validation, which finds what TOML's grammar allows and the
    /// language doesn't: a key defined twice, or a table or array of tables
    /// over a key already defined as something else.
    fn check(&self, _tree: &Tree, source: &str) -> Result<(), Diagnostic> {
        let parse = taplo::parser::parse(source);
        // Syntax errors are `collect_errors`'s to report.
        if !parse.errors.is_empty() {
            return Ok(());
        }
        let Err(mut errors) = parse.into_dom().validate() else {
            return Ok(());
        };
        errors
            .next()
            .map_or(Ok(()), |error| Err(dom_diagnostic(&error, source)))
    }
}

/// taplo's message for a DOM error, with the keys it names, at the line of the
/// definition that conflicts with an earlier one.
fn dom_diagnostic(error: &Error, source: &str) -> Diagnostic {
    let converter = LineConverter::new(source);
    let line_at = |range: TextRange| converter.textsize_to_line(range.start()) + 1;
    let line_of = |key: &Key| key.text_ranges().next().map(line_at);
    let on_line =
        |key: &Key| line_of(key).map_or_else(String::new, |line| format!(" on line {line}"));
    let (message, line) = match error {
        Error::ConflictingKeys { key, other } => (
            format!(
                "{error}: `{}` is already defined{}",
                key.value(),
                on_line(other)
            ),
            line_of(key),
        ),
        Error::ExpectedTable {
            not_table,
            required_by,
        } => (
            format!(
                "{error}: `{}`{} is not a table",
                not_table.value(),
                on_line(not_table)
            ),
            line_of(required_by),
        ),
        Error::ExpectedArrayOfTables {
            not_array_of_tables,
            required_by,
        } => (
            format!(
                "{error}: `{}`{} is not an array of tables",
                not_array_of_tables.value(),
                on_line(not_array_of_tables)
            ),
            line_of(required_by),
        ),
        Error::InvalidEscapeSequence { string } => {
            (error.to_string(), Some(line_at(string.text_range())))
        }
        // Its message carries a dump of the syntax tree.
        Error::UnexpectedSyntax { syntax } => (
            String::from("the syntax was not expected here"),
            Some(line_at(syntax.text_range())),
        ),
        Error::Query(_) => (error.to_string(), None),
    };
    Diagnostic {
        message,
        line,
        column: None,
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
