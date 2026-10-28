//! Layout: the whitespace an edit's `content` lands with — its indentation, and
//! the seams where it meets the file.
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
//! columns and valid read dedented. Elsewhere only the evidence's reading is
//! tried ([`Readings`]).
//!
//! The "first line" is the first line with code on it. Blank lines before it are
//! the content's own leading blank lines (an insert's `\n\n`), not where its code
//! begins, so they don't make that code a continuation line.
//!
//! **Seams.** Where the content meets the file ([`Seam`]):
//!
//! - An insert next to an item that occupies whole lines goes on lines of its own
//!   at that item's column, including after a `,` or `;` (and a trailing comment)
//!   that ends the item's line. An insert inside a line stays inline.
//! - Blank lines between the new code and its neighbors come from the content:
//!   the seam adds the one line break the content's own text lacks, never more.
//! - Inserted text takes the file's line endings.

use crate::editor::{Edit, Editor};
use tree_sitter::{InputEdit, Node, Parser, Point, Tree};

/// What layout needs to know about a language.
#[derive(Clone, Copy, Debug)]
pub struct Rules {
    /// The node kinds a content line can begin inside of without its leading
    /// whitespace being layout: string literals. Such a line is never moved.
    pub(crate) literals: &'static [&'static str],
    /// Which readings of the content's columns become candidates.
    pub(crate) readings: Readings,
    /// The node kinds that hold statements, where a statement can end at the same
    /// byte as the nodes inside it (Python's `block` and `module`). An insert
    /// after any of those nodes follows the statement, at its column.
    pub(crate) statements: &'static [&'static str],
    /// Prose rather than code: a line ends only where its text does (a `,` or `;`
    /// there is punctuation, not a separator), and an insert after a line that
    /// begins the way words follow one another (` more`, `, more`) continues that
    /// line rather than starting a new one.
    pub(crate) prose: bool,
}

impl Rules {
    /// Code whose indentation isn't syntax and that has no multi-line literals.
    pub(crate) const CODE: Self = Self {
        literals: &[],
        readings: Readings::Evidence,
        statements: &[],
        prose: false,
    };

    /// Plain text and Markdown: prose, whose indentation (list nesting, indented
    /// code) is too meaningful to rebase, so content is always read as written.
    pub(crate) const PROSE: Self = Self {
        literals: &[],
        readings: Readings::AsWritten,
        statements: &[],
        prose: true,
    };
}

/// Which readings of content's columns a language tries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Readings {
    /// Indentation is syntax: both readings are candidates, the evidence's first,
    /// and validation chooses.
    Both,
    /// Only the reading the evidence picks. Where indentation isn't syntax, the
    /// other reading would always validate too, so it could only ever be chosen by
    /// accident.
    Evidence,
    /// Always at file columns: continuation lines are never moved.
    AsWritten,
}

/// How content meets the source where it is spliced.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Seam {
    /// The splice point is inside a line, so the content's first line continues
    /// that line.
    Inline,
    /// The splice point is the first token on its line, and the content replaces
    /// text from there: its first line takes that line.
    LineStart,
    /// The splice point is where its line's text begins (the first token, or the
    /// end of a blank line or of the file), and nothing is replaced: the content
    /// goes on lines of its own above that line, which keeps its indentation.
    /// Content ending in a space or tab is a prefix of the line instead
    /// (`async `), and stays on it.
    BeforeLine,
    /// The splice point ends a line holding an item that occupies whole lines, and
    /// nothing is replaced: the content goes on lines of its own below it, at the
    /// item's column.
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
    /// A separator that led the content, kept on the line the content goes below
    /// ([`Seam::AfterLine`]), ahead of the line break.
    lead: &'a str,
    /// The file's line ending, which every line break the layout writes takes.
    newline: &'static str,
}

impl<'a> Target<'a> {
    /// A replacement of text starting at `start`, laid out against the column of
    /// the line `start` is on.
    pub(crate) fn replacing(source: &'a str, start: usize) -> Self {
        let seam = if is_indentation(&source[line_start(source, start)..start]) {
            Seam::LineStart
        } else {
            Seam::Inline
        };
        Self {
            source,
            start,
            seam,
            indent: line_indentation(source, start),
            lead: "",
            newline: line_ending(source),
        }
    }

    /// An insert of `content` at `start`, and the part of `content` to lay out
    /// there. `item` is where the code an insert at `start` follows begins, when
    /// the candidate knows: the content goes below that code's line, at its
    /// column, if the code occupies whole lines.
    ///
    /// The geometry decides, not the operation: an insert where only indentation
    /// precedes `start` goes above that line (`insert_before`, and `insert_after`
    /// a node that ends with its line's break, like a TOML table); one whose line
    /// ends at `start` goes below it; anything else is inside a line.
    pub(crate) fn inserting(
        source: &'a str,
        start: usize,
        item: Option<usize>,
        tree: Option<&Tree>,
        rules: &Rules,
        content: &'a str,
    ) -> (Self, &'a str) {
        let target = |start, seam, indent| Self {
            source,
            start,
            seam,
            indent,
            lead: "",
            newline: line_ending(source),
        };

        if is_indentation(&source[line_start(source, start)..start]) {
            // Inserting anywhere in a line's indentation is inserting before its
            // first token.
            let token = start + indentation(&source[start..]).len();
            let indent = line_indentation(source, token);
            return (target(token, Seam::BeforeLine, indent), content);
        }

        let inline = target(start, Seam::Inline, line_indentation(source, start));
        let Some(item) =
            item.filter(|item| is_indentation(&source[line_start(source, *item)..*item]))
        else {
            return (inline, content);
        };
        let Some(end) = end_of_line_after(source, start, tree, rules.prose) else {
            return (inline, content);
        };
        if rules.prose && continues_sentence(content) {
            return (inline, content);
        }

        let indent = line_indentation(source, item);
        if !rules.prose && ends_list_element(source, start) {
            if let Some(rest) = content.strip_prefix([',', ';']) {
                // The content's separator ends the line it follows, before any
                // separator that line already had, which then follows the content.
                let target = Self {
                    lead: &content[..1],
                    ..target(start, Seam::AfterLine, indent)
                };
                return (target, rest.trim_start_matches([' ', '\t']));
            }
        }
        (target(end, Seam::AfterLine, indent), content)
    }
}

/// Whether prose `content` inserted after a line continues it: it begins the way
/// words follow one another, with a single space or a `,` or `;`. Other leading
/// whitespace is indentation, beginning a line of its own (a nested list item).
fn continues_sentence(content: &str) -> bool {
    match content.strip_prefix(' ') {
        Some(rest) => !rest.starts_with([' ', '\t']),
        None => content.starts_with([',', ';']),
    }
}

/// Whether the code ending at `start` is an element of a separated list: its
/// separator follows it, or the list's closing bracket does. Only there does a
/// separator leading the content belong to it; after a statement or a whole
/// definition a `,` can still parse (as a Python tuple), but means something else.
fn ends_list_element(source: &str, start: usize) -> bool {
    source[start..]
        .trim_start()
        .starts_with([',', ';', ')', ']', '}'])
}

/// Where content goes below the line `start` ends: `start` itself when the rest
/// of its line is blank, or past a `,`/`;` and a trailing comment that end the
/// line too. `None` when more of the line follows, so `start` is inside it.
fn end_of_line_after(
    source: &str,
    start: usize,
    tree: Option<&Tree>,
    prose: bool,
) -> Option<usize> {
    let end = line_end(source, start);
    let rest = &source[start..end];
    if is_indentation(rest) {
        return Some(start);
    }
    if prose {
        return None;
    }
    let rest = rest.trim_start_matches([' ', '\t']);
    let (after_separator, rest) = match rest.strip_prefix([',', ';']) {
        Some(rest) => (end - rest.len(), rest),
        None => (start, rest),
    };
    if is_indentation(rest) {
        return Some(after_separator);
    }
    let comment = end - rest.trim_start_matches([' ', '\t']).len();
    trailing_comment(source, tree?, comment, end).then_some(end)
}

/// Whether a comment starts at `at` and runs to the end of its line, which ends
/// at `end` (the comment node may take the line break too, as Rust's doc
/// comments do).
fn trailing_comment(source: &str, tree: &Tree, at: usize, end: usize) -> bool {
    let text_end = at + source[at..end].trim_end().len();
    let mut node = tree.root_node().descendant_for_byte_range(at, at + 1);
    while let Some(current) = node.filter(|node| node.start_byte() == at) {
        if current.kind().contains("comment") {
            return current.end_byte() >= text_end
                && source
                    .get(text_end..current.end_byte())
                    .is_some_and(|rest| rest.trim().is_empty());
        }
        node = current.parent();
    }
    false
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

impl Reading {
    fn other(self) -> Self {
        match self {
            Self::FileColumns => Self::Dedented,
            Self::Dedented => Self::FileColumns,
        }
    }
}

/// An edit's content, split into lines for layout.
#[derive(Debug)]
pub(crate) struct Content<'a> {
    /// Without their line endings, whichever the content used.
    lines: Vec<&'a str>,
    /// The first line with code on it: the line placed at the target's column.
    first: Option<usize>,
    /// Which lines begin inside a string literal (see [`Content::set_verbatim`]).
    verbatim: Vec<bool>,
}

impl<'a> Content<'a> {
    pub(crate) fn new(text: &'a str) -> Self {
        let lines = text
            .split('\n')
            .map(|line| line.strip_suffix('\r').unwrap_or(line))
            .collect::<Vec<_>>();
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
    /// columns. `convention` is the reading the anchor shows the caller writes in
    /// (see [`anchor_convention`]), which takes precedence when there is one. The
    /// other reading follows only when it renders differently, and never for a
    /// first line that brings its own indentation: that states the content's
    /// columns outright.
    pub(crate) fn readings(
        &self,
        target: &Target<'_>,
        convention: Option<Reading>,
    ) -> Vec<Reading> {
        let column = width(target.indent);
        let leftmost = self
            .continuation_lines()
            .map(|line| width(indentation(line)))
            .min();
        match leftmost {
            Some(leftmost) if column > 0 && !self.first_line_indented(target.seam) => {
                let evidence = if leftmost < column {
                    Reading::Dedented
                } else {
                    Reading::FileColumns
                };
                let first = convention.unwrap_or(evidence);
                vec![first, first.other()]
            }
            _ => vec![Reading::FileColumns],
        }
    }

    /// Lay the content out at `target` under `reading`.
    pub(crate) fn render(&self, target: &Target<'_>, reading: Reading) -> Placement {
        let Target {
            source,
            indent,
            newline,
            ..
        } = *target;
        let bare_first_line = self.first == Some(0) && !has_indentation(self.lines[0]);
        // Where the text goes in, and whether its first line begins a line of the
        // result (and so is placed at the target's column) or continues one.
        let (start, mut text, first_begins_line) = match target.seam {
            Seam::Inline => (target.start, String::new(), false),
            Seam::AfterLine => {
                let mut text = String::from(target.lead);
                // The content goes below the line it follows. A line break the
                // content begins with is that one, and any after it are blank
                // lines it asked for; otherwise the seam supplies it.
                if self.first == Some(0) {
                    text.push_str(newline);
                }
                (target.start, text, true)
            }
            // A bare first line goes in after the target line's own indentation.
            Seam::LineStart | Seam::BeforeLine if bare_first_line => {
                (target.start, String::new(), false)
            }
            // A first line with its own indentation, or leading blank lines, takes
            // the whole line.
            Seam::LineStart | Seam::BeforeLine => {
                (line_start(source, target.start), String::new(), true)
            }
        };

        let mut line_offsets = Vec::with_capacity(self.lines.len());
        for (index, line) in self.lines.iter().enumerate() {
            if index > 0 {
                text.push_str(newline);
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

        let last = self.lines.len() - 1;
        let ends_with_blank_line = last > 0 && is_blank(self.lines[last]) && !self.verbatim[last];
        match target.seam {
            // The line the content goes above keeps its indentation, whatever
            // whitespace the content ends with: an insert doesn't move its anchor.
            Seam::BeforeLine if ends_with_blank_line => {
                text.truncate(line_offsets[last]);
                if start == target.start {
                    text.push_str(indent);
                }
            }
            // Content that ends with code goes on lines of its own above the line,
            // unless it ends in a space or tab: then it is a prefix of the line.
            Seam::BeforeLine
                if !is_blank(self.lines[last]) && !self.lines[last].ends_with([' ', '\t']) =>
            {
                text.push_str(newline);
                if start == target.start {
                    text.push_str(indent);
                }
            }
            // The line break that ends the line the content follows now follows
            // the content, so a blank last line's whitespace would trail it.
            Seam::AfterLine if ends_with_blank_line => text.truncate(line_offsets[last]),
            _ => {}
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
    /// where the text goes in: the target's start, the start of its line when
    /// the content takes the whole line, or the end of the line it goes below
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

/// Lay out each candidate's content at its target, in order (see the module
/// docs). A candidate that lands where an earlier one did, with the same text, is
/// dropped: candidates found through different nodes often land identically, and
/// trying one again would only rerun validation.
pub(crate) fn lay_out<'editor, 'language>(
    editor: &'editor Editor<'language>,
    edits: Vec<Edit<'editor, 'language>>,
) -> Vec<Edit<'editor, 'language>> {
    let rules = editor.language().editor().layout();
    let mut parser = (!rules.literals.is_empty())
        .then(|| editor.language().tree_sitter_parser().ok())
        .flatten();
    let context = Context {
        rules,
        tree: editor.tree(),
        anchor: editor.selector().anchor(),
    };

    let mut laid_out: Vec<Edit<'editor, 'language>> = Vec::with_capacity(edits.len());
    for edit in edits {
        for candidate in lay_out_edit(edit, &context, parser.as_mut()) {
            if !laid_out.iter().any(|seen| {
                seen.position() == candidate.position() && seen.content() == candidate.content()
            }) {
                laid_out.push(candidate);
            }
        }
    }
    laid_out
}

/// What laying out one candidate needs besides the candidate.
struct Context<'a> {
    rules: Rules,
    /// The parse of the file the candidates edit.
    tree: &'a Tree,
    /// The anchor as the caller wrote it, if the edit has one.
    anchor: Option<&'a str>,
}

/// `edit`, laid out at its target: one candidate per reading the language tries.
fn lay_out_edit<'editor, 'language>(
    edit: Edit<'editor, 'language>,
    context: &Context<'_>,
    parser: Option<&mut Parser>,
) -> Vec<Edit<'editor, 'language>> {
    let rules = &context.rules;
    let source = edit.source_code();
    let start = edit.start_byte();
    let end = edit.position().end_byte;
    if let Some(end) = end.filter(|_| edit.content().is_empty()) {
        let (start, end) = deletion(source, start, end);
        return vec![edit.with_start_byte(start).with_end_byte(end)];
    }
    let (target, text) = match end {
        Some(end) => (
            Target::replacing(source, start),
            through_line_end(edit.content(), source, end, context.anchor),
        ),
        None => Target::inserting(
            source,
            start,
            item_start(&edit, rules.statements),
            Some(context.tree),
            rules,
            edit.content(),
        ),
    };

    let mut content = Content::new(text);
    if let Some(parser) = parser {
        if content.line_count() > 1 {
            mark_literal_lines(
                &mut content,
                &target,
                context.tree,
                end,
                parser,
                rules.literals,
            );
        }
    }

    let mut readings = match rules.readings {
        Readings::AsWritten => vec![Reading::FileColumns],
        _ => {
            let hit = edit.anchor_hit().copied();
            content.readings(&target, anchor_convention(context.anchor, source, hit))
        }
    };
    if rules.readings == Readings::Evidence {
        readings.truncate(1);
    }

    readings
        .into_iter()
        .map(|reading| {
            let placement = content.render(&target, reading);
            edit.clone()
                .with_start_byte(placement.start())
                .with_content(placement.into_text())
        })
        .collect()
}

/// What deleting `start..end` removes. Code that occupies whole lines goes with
/// those lines, line breaks and all, so no empty line is left in its place. Then
/// the blank lines that separated it from its neighbors would double up: of the
/// runs on either side, the longer stays, as the separation the code around it
/// uses, and neither stays at the start of the file or before the end of a block
/// or the file. A deletion inside a line removes just its text.
fn deletion(source: &str, start: usize, end: usize) -> (usize, usize) {
    let first = line_start(source, start);
    // A node that ends with its line's break (a Rust doc comment, a TOML table)
    // ends on the line it breaks.
    let through = match source[start..end].strip_suffix('\n') {
        Some(text) => start + text.strip_suffix('\r').unwrap_or(text).len(),
        None => end,
    };
    let last = line_end(source, through);
    if !is_indentation(&source[first..start]) || !is_indentation(&source[through..last]) {
        return (start, end);
    }
    let (from, to) = (first, after_line_break(source, last));

    let before = blank_lines_before(source, from);
    let after = blank_lines_after(source, to);
    let rest = source[after..].trim_start_matches([' ', '\t']);
    if before == 0 || rest.is_empty() || rest.starts_with(['}', ']', ')']) {
        return (before, after);
    }
    let count = |text: &str| text.matches('\n').count();
    match (count(&source[before..from]), count(&source[to..after])) {
        (0, _) | (_, 0) => (from, to),
        (before_run, after_run) if after_run > before_run => (before, to),
        _ => (from, after),
    }
}

/// The start of the blank lines that end at `line`, a line's start.
fn blank_lines_before(source: &str, line: usize) -> usize {
    let mut start = line;
    while start > 0 {
        let previous = line_start(source, start - 1);
        if !is_blank(&source[previous..start]) {
            break;
        }
        start = previous;
    }
    start
}

/// The end of the blank lines that begin at `line`, a line's start.
fn blank_lines_after(source: &str, line: usize) -> usize {
    let mut end = line;
    while end < source.len() {
        let next = after_line_break(source, line_end(source, end));
        if !is_blank(&source[end..next]) {
            break;
        }
        end = next;
    }
    end
}

/// The start of the line after the one ending at `end`, or the end of the file.
fn after_line_break(source: &str, end: usize) -> usize {
    let rest = &source[end..];
    end + if rest.starts_with("\r\n") {
        2
    } else {
        usize::from(rest.starts_with('\n'))
    }
}

/// `content` for a replacement that ends at `end`, without the line break it ends
/// with when that break stands for the target line's own: when the anchor ends
/// with a line break too, and nothing but whitespace follows `end` on its line.
/// Anchors are matched without their surrounding whitespace, so the line's own
/// break is still there after the replacement, and the content's would add a
/// blank line neither of them has.
fn through_line_end<'a>(
    content: &'a str,
    source: &str,
    end: usize,
    anchor: Option<&str>,
) -> &'a str {
    let ends_line = |text: &str| text.trim_end_matches([' ', '\t']).ends_with('\n');
    if anchor.is_some_and(ends_line)
        && ends_line(content)
        && is_indentation(&source[end..line_end(source, end)])
    {
        let content = content.trim_end_matches([' ', '\t']);
        let content = &content[..content.len() - 1];
        content.strip_suffix('\r').unwrap_or(content)
    } else {
        content
    }
}

/// Where the code an insert at the edit's start follows begins: the statement it
/// ends, in a language whose statements end where their last nodes do (Python);
/// else the first of the candidate's nodes; else its anchor match.
fn item_start(edit: &Edit<'_, '_>, statements: &[&str]) -> Option<usize> {
    if let Some(nodes) = edit.nodes() {
        let statement = statement_ending_at(nodes, edit.start_byte(), statements);
        if let Some(node) = statement.or_else(|| nodes.first().copied()) {
            return Some(node.start_byte());
        }
    }
    edit.anchor_hit().map(|(start, _)| *start)
}

/// The statement that ends at `start` when the last of `nodes` does: climbing the
/// ancestors that also end there, the one whose parent is one of `statements`.
/// The node an anchor resolves to is often a sub-expression (the `identifier` in
/// an assignment), which ends where its statement does.
fn statement_ending_at<'tree>(
    nodes: &[Node<'tree>],
    start: usize,
    statements: &[&str],
) -> Option<Node<'tree>> {
    let mut current = *nodes.last()?;
    while current.end_byte() == start {
        let parent = current.parent()?;
        if statements.contains(&parent.kind()) {
            return Some(current);
        }
        current = parent;
    }
    None
}

/// The reading the anchor shows the content was written in, when its own
/// continuation lines leave the target's column: each at exactly the column of
/// the line of the file it matched, and at least one left of where the anchor
/// begins. An edit spanning out of its target's block (the rest of a method and
/// the module-level function after it) reads as dedented by its continuation
/// lines alone; an anchor copied at file columns across the same lines says
/// otherwise. `None` when the anchor has no such lines to show.
fn anchor_convention(
    anchor: Option<&str>,
    source: &str,
    hit: Option<(usize, usize)>,
) -> Option<Reading> {
    let (start, end) = hit?;
    let anchor = anchor?.trim();
    let column = width(line_indentation(source, start));
    let anchor_lines = anchor.split('\n').collect::<Vec<_>>();
    let source_lines = source[start..end].split('\n').collect::<Vec<_>>();
    if anchor_lines.len() != source_lines.len() {
        return None;
    }
    let mut leaves = false;
    for (written, matched) in anchor_lines.iter().zip(&source_lines).skip(1) {
        if is_blank(written) {
            continue;
        }
        let written = width(indentation(written));
        if written != width(indentation(matched)) {
            return None;
        }
        leaves |= written < column;
    }
    leaves.then_some(Reading::FileColumns)
}

/// Mark which of `content`'s lines would begin inside a node of one of `kinds`
/// (string literals), from a parse of the edit's result, so a line is recognized
/// whether its literal opens in the content or in the file around it (an edit
/// inside a multi-line string).
///
/// Re-indenting lines outside literals can't move a literal's quotes, so any
/// reading's parse finds the same literals — except that a parser recovering
/// from an error may not scan string contents (tree-sitter's Python scanner
/// doesn't). If the file-columns parse has one, the dedented reading is parsed
/// too, and unless that parse is clean, a line either one places in a literal is
/// kept as written. If nothing parses, no line is moved.
fn mark_literal_lines(
    content: &mut Content<'_>,
    target: &Target<'_>,
    tree: &Tree,
    end: Option<usize>,
    parser: &mut Parser,
    kinds: &[&str],
) {
    let mut parse = |content: &Content<'_>, reading| {
        let placement = content.render(target, reading);
        let tree = placement.reparse(parser, tree, target.source, end)?;
        Some((
            placement.lines_inside(&tree, kinds),
            tree.root_node().has_error(),
        ))
    };

    let unknown = vec![true; content.line_count()];
    let verbatim = match parse(content, Reading::FileColumns) {
        None => unknown,
        Some((verbatim, false)) => verbatim,
        Some((verbatim, true)) => {
            content.set_verbatim(verbatim.clone());
            match parse(content, Reading::Dedented) {
                None => unknown,
                Some((dedented, false)) => dedented,
                Some((dedented, true)) => verbatim
                    .into_iter()
                    .zip(dedented)
                    .map(|(either, or)| either || or)
                    .collect(),
            }
        }
    };
    content.set_verbatim(verbatim);
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

/// The line ending `source` uses: that of its first line, `\n` when it has
/// only one line.
pub(crate) fn line_ending(source: &str) -> &'static str {
    match source.find('\n') {
        Some(newline) if source[..newline].ends_with('\r') => "\r\n",
        _ => "\n",
    }
}

fn line_start(source: &str, byte: usize) -> usize {
    source[..byte].rfind('\n').map_or(0, |newline| newline + 1)
}

/// The end of the line `byte` is on, before its line ending.
fn line_end(source: &str, byte: usize) -> usize {
    let end = source[byte..]
        .find('\n')
        .map_or(source.len(), |newline| byte + newline);
    if end > byte && source[..end].ends_with('\r') {
        end - 1
    } else {
        end
    }
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
mod tests;
