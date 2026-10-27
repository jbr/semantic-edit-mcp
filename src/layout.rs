//! Layout: the indentation an edit's `content` lands with.
//!
//! This is the content contract from the design (`docs/design/formatting.md`,
//! "Layout"). Agents write `content` the way they copy anchors: at the file's
//! columns, usually leaving off the first line's indentation, because an anchor's
//! first line matches from its first non-whitespace character. So:
//!
//! - **Content is in file columns.** A first line without leading whitespace is
//!   placed at the target's column. Leading whitespace on the first line replaces
//!   the target line's indentation.
//! - **Continuation lines are kept as written**, unless the content is *evidently
//!   dedented*: its continuation lines sit left of the target's column. Then it
//!   reads as written relative to column 0, and each continuation line moves under
//!   the target's indentation.
//! - **A line that begins inside a string literal is never re-indented.**
//!
//! [`Content::readings`] orders the two readings by that evidence. Where
//! indentation is syntax, a language tries both and lets validation choose:
//! `if x:\n    y = 1` replacing a statement at column 4 is invalid read at file
//! columns and valid read dedented.
//!
//! The "first line" is the first line with code on it. Blank lines before it are
//! the content's own leading blank lines (an insert's `\n\n`), not where its code
//! begins, so they don't make that code a continuation line.

use tree_sitter::{InputEdit, Parser, Point, Tree};

/// How content meets the source where it is spliced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Seam {
    /// The splice point follows other code on its line, so the content's first
    /// line continues that line.
    Inline,
    /// The splice point is the first token on its line, and the content replaces
    /// text from there: its first line takes that line.
    LineStart,
    /// The splice point is the first token on its line, and nothing is replaced:
    /// the content goes on lines of its own above that line, which keeps its
    /// indentation.
    BeforeLine,
    /// The splice point ends a line, and nothing is replaced: the content goes on
    /// lines of its own below it.
    AfterLine,
}

/// Where content is spliced, and the column it is laid out against.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Target<'a> {
    source: &'a str,
    start: usize,
    seam: Seam,
    /// The target's column, as the indentation that reaches it.
    indent: &'a str,
}

impl<'a> Target<'a> {
    /// A splice at `start`, laid out against the column of the line `start` is
    /// on. `replacing` says whether the edit replaces text from `start`; an insert
    /// at the first token of a line goes on lines of its own above it.
    pub(crate) fn at(source: &'a str, start: usize, replacing: bool) -> Self {
        let seam = if is_indentation(&source[line_start(source, start)..start]) {
            if replacing {
                Seam::LineStart
            } else {
                Seam::BeforeLine
            }
        } else {
            Seam::Inline
        };
        Self {
            source,
            start,
            seam,
            indent: line_indentation(source, start),
        }
    }

    /// Content on lines of its own after `start`, which ends a line, laid out
    /// against the column of the line holding `column_of` — the start of the
    /// statement `start` ends, which is not the line `start` is on when that
    /// statement spans several.
    pub(crate) fn after_line(source: &'a str, start: usize, column_of: usize) -> Self {
        Self {
            source,
            start,
            seam: Seam::AfterLine,
            indent: line_indentation(source, column_of),
        }
    }
}

/// A way of reading the columns content was written in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Reading {
    /// At the file's columns: continuation lines are kept exactly as written.
    FileColumns,
    /// Relative to column 0, as if the target were there: each continuation
    /// line moves under the target's indentation.
    Dedented,
}

/// An edit's content, split into lines for layout.
#[derive(Debug)]
pub(crate) struct Content<'a> {
    lines: Vec<&'a str>,
    /// The first line with code on it: the line placed at the target's column.
    first: Option<usize>,
    /// Which lines begin inside a string literal (see [`Content::set_verbatim`]).
    verbatim: Vec<bool>,
}

impl<'a> Content<'a> {
    pub(crate) fn new(text: &'a str) -> Self {
        let lines = text.split('\n').collect::<Vec<_>>();
        Self {
            first: lines.iter().position(|line| !is_blank(line)),
            verbatim: vec![false; lines.len()],
            lines,
        }
    }

    pub(crate) fn line_count(&self) -> usize {
        self.lines.len()
    }

    /// Mark which lines begin inside a string literal, one flag per line. Such a
    /// line's leading whitespace belongs to the literal, so no reading moves it,
    /// and it says nothing about the columns the content was written in.
    pub(crate) fn set_verbatim(&mut self, verbatim: Vec<bool>) {
        debug_assert_eq!(verbatim.len(), self.lines.len());
        self.verbatim = verbatim;
    }

    /// The readings worth rendering at `target`, the evidence's first.
    ///
    /// The content is evidently dedented when its continuation lines' minimum
    /// indentation is left of the target's column; otherwise it is at file
    /// columns. The other reading follows only when it renders differently, and
    /// never for a first line that brings its own indentation: that states the
    /// content's columns outright. A caller for which indentation isn't syntax
    /// takes only the first.
    pub(crate) fn readings(&self, target: &Target<'_>) -> Vec<Reading> {
        let column = width(target.indent);
        let leftmost = self
            .continuation_lines()
            .map(|line| width(indentation(line)))
            .min();
        match leftmost {
            Some(leftmost) if column > 0 && !self.first_line_indented(target.seam) => {
                if leftmost < column {
                    vec![Reading::Dedented, Reading::FileColumns]
                } else {
                    vec![Reading::FileColumns, Reading::Dedented]
                }
            }
            _ => vec![Reading::FileColumns],
        }
    }

    /// Lay the content out at `target` under `reading`.
    pub(crate) fn render(&self, target: &Target<'_>, reading: Reading) -> Placement {
        let indent = target.indent;
        let bare_first_line = self.first == Some(0) && !has_indentation(self.lines[0]);
        // Where the text goes in, and whether its first line begins a line of the
        // result (and so is placed at the target's column) or continues one.
        let (start, mut text, first_begins_line) = match target.seam {
            Seam::Inline => (target.start, String::new(), false),
            Seam::AfterLine => (target.start, String::from("\n"), true),
            // A bare first line goes in after the target line's own indentation.
            Seam::LineStart | Seam::BeforeLine if bare_first_line => {
                (target.start, String::new(), false)
            }
            // A first line with its own indentation, or leading blank lines, takes
            // the whole line.
            Seam::LineStart | Seam::BeforeLine => {
                (line_start(target.source, target.start), String::new(), true)
            }
        };

        let mut line_offsets = Vec::with_capacity(self.lines.len());
        for (index, line) in self.lines.iter().enumerate() {
            if index > 0 {
                text.push('\n');
            }
            line_offsets.push(text.len());
            let indented = match self.first {
                Some(first) if index == first => {
                    (index > 0 || first_begins_line) && !has_indentation(line)
                }
                Some(first) if index > first => {
                    reading == Reading::Dedented && self.is_continuation(index)
                }
                _ => false,
            };
            if indented {
                text.push_str(indent);
            }
            text.push_str(line);
        }

        // The line the content goes above keeps its indentation, whatever
        // whitespace the content ends with: an insert doesn't move its anchor.
        let last = self.lines.len() - 1;
        if target.seam == Seam::BeforeLine
            && last > 0
            && is_blank(self.lines[last])
            && !self.verbatim[last]
        {
            text.truncate(line_offsets[last]);
            if start == target.start {
                text.push_str(indent);
            }
        }

        Placement {
            start,
            text,
            line_offsets,
        }
    }

    /// Whether the first line is laid out at the start of a line of the result
    /// and brings its own indentation there.
    fn first_line_indented(&self, seam: Seam) -> bool {
        self.first.is_some_and(|first| {
            has_indentation(self.lines[first]) && (first > 0 || seam != Seam::Inline)
        })
    }

    /// Lines after the first line whose indentation is layout: neither blank nor
    /// inside a string literal.
    fn continuation_lines(&self) -> impl Iterator<Item = &'a str> + '_ {
        self.lines
            .iter()
            .enumerate()
            .filter(|(index, _)| self.first.is_some_and(|first| *index > first))
            .filter(|(index, _)| self.is_continuation(*index))
            .map(|(_, line)| *line)
    }

    fn is_continuation(&self, index: usize) -> bool {
        !is_blank(self.lines[index]) && !self.verbatim[index]
    }
}

/// Content laid out at a target: `text` goes in at `start`, replacing through the
/// edit's end if it has one.
#[derive(Debug, fieldwork::Fieldwork)]
#[fieldwork(get, into_field)]
pub(crate) struct Placement {
    /// where the text goes in: the target's start, or the start of its line when
    /// the content takes the whole line
    start: usize,
    /// the laid-out content
    text: String,
    #[field(skip)]
    line_offsets: Vec<usize>,
}

impl Placement {
    /// The source with this placement spliced in, replacing through `end` (an
    /// insert has none).
    pub(crate) fn splice(&self, source: &str, end: Option<usize>) -> String {
        [&source[..self.start], &self.text, &source[self.end(end)..]].concat()
    }

    /// A parse of [`splice`](Self::splice)'s result, reusing `tree`, the parse
    /// of `source`, for everything the splice leaves alone.
    pub(crate) fn reparse(
        &self,
        parser: &mut Parser,
        tree: &Tree,
        source: &str,
        end: Option<usize>,
    ) -> Option<Tree> {
        let old_end = self.end(end);
        let start_position = point_at(source, self.start);
        let mut edited = tree.clone();
        edited.edit(&InputEdit {
            start_byte: self.start,
            old_end_byte: old_end,
            new_end_byte: self.start + self.text.len(),
            start_position,
            old_end_position: point_at(source, old_end),
            new_end_position: advance(start_position, &self.text),
        });
        parser.parse(self.splice(source, end), Some(&edited))
    }

    /// For each content line, whether it begins strictly inside a node of one of
    /// `kinds` in `tree`, a parse of [`splice`](Self::splice).
    pub(crate) fn lines_inside(&self, tree: &Tree, kinds: &[&str]) -> Vec<bool> {
        self.line_offsets
            .iter()
            .map(|offset| inside(tree, self.start + offset, kinds))
            .collect()
    }

    fn end(&self, end: Option<usize>) -> usize {
        end.map_or(self.start, |end| end.max(self.start))
    }
}

fn point_at(source: &str, byte: usize) -> Point {
    Point {
        row: source[..byte].matches('\n').count(),
        column: byte - line_start(source, byte),
    }
}

/// Where writing `text` at `from` ends.
fn advance(from: Point, text: &str) -> Point {
    match text.rfind('\n') {
        Some(newline) => Point {
            row: from.row + text.matches('\n').count(),
            column: text.len() - newline - 1,
        },
        None => Point {
            row: from.row,
            column: from.column + text.len(),
        },
    }
}

fn inside(tree: &Tree, byte: usize, kinds: &[&str]) -> bool {
    let mut node = tree.root_node().descendant_for_byte_range(byte, byte);
    while let Some(current) = node {
        if current.start_byte() < byte
            && byte < current.end_byte()
            && kinds.contains(&current.kind())
        {
            return true;
        }
        node = current.parent();
    }
    false
}

fn line_start(source: &str, byte: usize) -> usize {
    source[..byte].rfind('\n').map_or(0, |newline| newline + 1)
}

/// The indentation of the line `byte` is on.
fn line_indentation(source: &str, byte: usize) -> &str {
    indentation(&source[line_start(source, byte)..])
}

/// A line's leading spaces and tabs.
fn indentation(line: &str) -> &str {
    &line[..line.len() - line.trim_start_matches([' ', '\t']).len()]
}

fn has_indentation(line: &str) -> bool {
    line.starts_with([' ', '\t'])
}

fn is_indentation(text: &str) -> bool {
    text.chars().all(|c| matches!(c, ' ' | '\t'))
}

fn is_blank(line: &str) -> bool {
    line.trim().is_empty()
}

/// The column an indentation reaches, with a tab advancing to the next multiple
/// of 8, as in Python's tokenizer. Content and file are compared, never
/// converted: a tab and spaces only meet here when they are mixed.
fn width(indentation: &str) -> usize {
    indentation.chars().fold(0, |column, c| match c {
        '\t' => column / 8 * 8 + 8,
        _ => column + 1,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Render every reading `readings` offers, in order, as `(start, text)`.
    fn render_all(target: Target<'_>, content: &str) -> Vec<(usize, String)> {
        let content = Content::new(content);
        content
            .readings(&target)
            .into_iter()
            .map(|reading| {
                let placement = content.render(&target, reading);
                (placement.start(), placement.into_text())
            })
            .collect()
    }

    const BODY: &str = "def f():\n    old()\n    keep()\n";

    #[test]
    fn a_bare_first_line_goes_in_after_the_line_indentation() {
        let start = BODY.find("old").unwrap();
        let target = Target::at(BODY, start, true);
        let readings = render_all(target, "new()\n    more()");
        assert_eq!(
            readings,
            [
                (start, "new()\n    more()".into()),
                (start, "new()\n        more()".into()),
            ]
        );
    }

    #[test]
    fn leading_whitespace_on_the_first_line_replaces_the_line_indentation() {
        let start = BODY.find("old").unwrap();
        let target = Target::at(BODY, start, true);
        let readings = render_all(target, "  new()\n    more()");
        assert_eq!(readings, [(start - 4, "  new()\n    more()".into())]);
    }

    #[test]
    fn continuation_lines_left_of_the_target_read_as_dedented_first() {
        let start = BODY.find("old").unwrap();
        let target = Target::at(BODY, start, true);
        let readings = render_all(target, "new()\nmore()");
        assert_eq!(
            readings,
            [
                (start, "new()\n    more()".into()),
                (start, "new()\nmore()".into()),
            ]
        );
    }

    #[test]
    fn verbatim_lines_neither_move_nor_count() {
        let start = BODY.find("old").unwrap();
        let target = Target::at(BODY, start, true);
        let mut content = Content::new("s = \"\"\"\\\nraw\n\"\"\"\nmore()");
        content.set_verbatim(vec![false, true, true, false]);
        let dedented = content.render(&target, Reading::Dedented);
        assert_eq!(dedented.text(), "s = \"\"\"\\\nraw\n\"\"\"\n    more()");

        // With only verbatim lines after it, there is nothing to read either way.
        let mut content = Content::new("s = \"\"\"\\\nraw\n\"\"\"");
        content.set_verbatim(vec![false, true, true]);
        assert_eq!(content.readings(&target), [Reading::FileColumns]);
    }

    #[test]
    fn an_insert_above_a_line_leaves_that_line_where_it_was() {
        let start = BODY.find("keep").unwrap();
        let target = Target::at(BODY, start, false);
        // However the content ends, the anchor's line keeps its indentation.
        for content in ["new()\n", "new()\n    ", "new()\n        "] {
            let placement = Content::new(content).render(&target, Reading::FileColumns);
            assert_eq!(
                placement.splice(BODY, None),
                "def f():\n    old()\n    new()\n    keep()\n",
                "{content:?}"
            );
        }
        // An inline prefix stays inline.
        let placement = Content::new("async ").render(&target, Reading::FileColumns);
        assert_eq!(
            placement.splice(BODY, None),
            "def f():\n    old()\n    async keep()\n"
        );
    }

    #[test]
    fn content_after_a_line_starts_on_its_own_line() {
        let start = BODY.find("old()").unwrap() + "old()".len();
        let target = Target::after_line(BODY, start, start);
        let placement = Content::new("new()\n    more()").render(&target, Reading::FileColumns);
        assert_eq!(
            placement.splice(BODY, None),
            "def f():\n    old()\n    new()\n    more()\n    keep()\n"
        );
    }

    #[test]
    fn leading_blank_lines_come_before_the_first_line() {
        let start = BODY.find("keep()").unwrap() + "keep()".len();
        let target = Target::after_line(BODY, start, start);
        let readings = render_all(target, "\n\ndef g():\n        pass");
        assert_eq!(
            readings[0],
            (start, "\n\n\n    def g():\n        pass".into())
        );
    }

    #[test]
    fn module_level_content_is_always_as_written() {
        let source = "x = 1\n";
        let target = Target::at(source, 0, true);
        assert_eq!(
            render_all(target, "if y:\n    z = 1"),
            [(0, "if y:\n    z = 1".into())]
        );
    }

    /// A wrong `InputEdit` doesn't fail: tree-sitter silently reuses subtrees at
    /// the wrong bytes, so the incremental parse must match a fresh one exactly.
    #[test]
    fn a_reparse_matches_a_fresh_parse() {
        fn nodes(tree: &Tree) -> Vec<(String, usize, usize, Point, Point)> {
            let mut nodes = vec![];
            let mut cursor = tree.walk();
            'walk: loop {
                let node = cursor.node();
                nodes.push((
                    node.kind().to_string(),
                    node.start_byte(),
                    node.end_byte(),
                    node.start_position(),
                    node.end_position(),
                ));
                if cursor.goto_first_child() || cursor.goto_next_sibling() {
                    continue;
                }
                while cursor.goto_parent() {
                    if cursor.goto_next_sibling() {
                        continue 'walk;
                    }
                }
                break nodes;
            }
        }

        let mut parser = Parser::new();
        parser
            .set_language(&tree_sitter_python::LANGUAGE.into())
            .unwrap();
        // Code after the splice is what an incremental parse reuses, and so where
        // a wrong edit range would show.
        let source = format!("{BODY}{}", "\n\ndef later():\n    return 1\n".repeat(20));
        let original = parser.parse(&source, None).unwrap();
        let old = BODY.find("old").unwrap();
        let keep = BODY.find("keep").unwrap();
        let cases = [
            (
                Target::at(&source, old, true),
                Some(old + "old()".len()),
                "s = \"\"\"\\\nx\n\"\"\"",
            ),
            (
                Target::at(&source, old, true),
                Some(old + "old()".len()),
                "  new()",
            ),
            (Target::at(&source, keep, false), None, "if x:\n    y()\n"),
            (Target::after_line(&source, old + 5, old), None, "\n\nz = 1"),
            (Target::at(&source, old + 4, false), None, "\n    1,\n"),
        ];
        for (target, end, content) in cases {
            let placement = Content::new(content).render(&target, Reading::FileColumns);
            let reparsed = placement
                .reparse(&mut parser, &original, &source, end)
                .unwrap();
            let fresh = parser.parse(placement.splice(&source, end), None).unwrap();
            assert_eq!(nodes(&reparsed), nodes(&fresh), "{content:?}");
        }
    }

    #[test]
    fn tabs_reach_the_next_multiple_of_eight() {
        assert_eq!(width("\t"), 8);
        assert_eq!(width("  \t"), 8);
        assert_eq!(width("\t  "), 10);
        assert_eq!(width("    "), 4);
    }
}
