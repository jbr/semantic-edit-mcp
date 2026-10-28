//! Keeping a formatter's changes to the edit (`docs/design/formatting.md`,
//! "Formatting": scope is the edit).
//!
//! The formatter formats the whole laid-out result, so it sees the edit in
//! context. Its output is then diffed against the laid-out result line by line,
//! and only the hunks that touch the lines the edit changed, or the line on
//! either side of them, are applied: the rest of the file stays as it was.
//! darker (for black), `git-clang-format` and VS Code's `formatOnSaveMode:
//! "modifications"` work the same way. Formatters' own range options are too
//! uneven to use instead: rustfmt's needs nightly, and biome has none.
//!
//! A formatter's changes can depend on each other. One that moves a line, as
//! rustfmt does to sort imports, removes it in one hunk and adds it in another,
//! and keeping one of the two would lose the line or duplicate it, in a file that
//! still parses. When the line is the edit's own and the hunk far from the edit
//! does nothing but put it in its new place, [`keep`] keeps both; any other move
//! across the boundary it refuses. Other dependencies leave the file invalid (a
//! brace moved from far away to the edit), so the caller validates the result
//! again.

use diffy::{DiffOptions, Hunk, Line};
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet},
    ops::Range,
};

/// The lines of `laid_out` the edit changed, as indexes from 0, end exclusive.
/// `laid_out` is `source` with the edit's text in place at `span`, a byte range
/// of `laid_out`.
///
/// They are the lines the edit's text is on, less a first or last line on which
/// that text is only whitespace and which reads exactly as a line of `source`
/// did: the line break that ends the line an insert goes below, or the
/// indentation given back to the line an insert goes above. A deletion's text is
/// empty, and the edit is the point where the text was removed: between two
/// lines, an empty range, when whole lines went, and otherwise the line it was
/// removed from.
pub(super) fn edited_lines(source: &str, laid_out: &str, span: Range<usize>) -> Range<usize> {
    let first = line_index(laid_out, span.start);
    if span.is_empty() {
        return if line_start(laid_out, span.start) == span.start {
            first..first
        } else {
            first..first + 1
        };
    }
    let mut lines = first..line_index(laid_out, span.end - 1) + 1;
    if lines.len() > 1 && seam_begins(source, laid_out, &span) {
        lines.start += 1;
    }
    if lines.len() > 1 && seam_ends(source, laid_out, &span) {
        lines.end -= 1;
    }
    lines
}

/// What [`keep`] keeps of a formatter's changes.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct Kept {
    /// The laid-out result with those changes made.
    pub(super) text: String,
    /// Whether one of them, away from the edit, puts text the formatter moved
    /// out of the edit in its new place.
    pub(super) moved: bool,
}

/// `laid_out` with those of `formatted`'s changes to it that touch `edited`, its
/// lines the edit changed (see [`edited_lines`]), or the line on either side of
/// them. Each line a kept change writes ends with `newline`, the file's line
/// ending.
///
/// A change is kept or left out whole. The formatter's hunks are what they are,
/// so one that straddles the boundary, reflowing a statement that begins before
/// the edit, say, is kept, and reaches a little beyond the edit.
///
/// Where the formatter moves text out of the edit, as rustfmt does to sort an
/// inserted import, the change that puts it in its new place is carried with
/// the changes kept, if it does nothing else ([`carry_moves`]). That change can
/// be anywhere in the file, but it only adds lines, with no words but the
/// edit's own: it places the edit's text where the formatter put it, and every
/// line already there stays as it was.
///
/// `None` if the formatter moved text across the boundary that can't be
/// carried: text that was already in the file, as when it moves a line from
/// elsewhere into the edit, or the edit's text to a place where it changes
/// other lines too. Then keeping either side of the move alone would lose the
/// text or duplicate it.
pub(super) fn keep(
    laid_out: &str,
    formatted: &str,
    edited: Range<usize>,
    newline: &str,
) -> Option<Kept> {
    // Compared without their line endings: rustfmt and biome write LF whatever
    // the file uses, so a CRLF file would otherwise differ on every line.
    let (old, new) = (lf(laid_out), lf(formatted));
    let patch = DiffOptions::new()
        .set_context_len(0)
        .create_patch(&old, &new);
    let hunks = patch.hunks();
    let near = edited.start.saturating_sub(1)..edited.end + 1;
    let mut kept = hunks
        .iter()
        .map(|hunk| touches(&replaced_lines(hunk), &near))
        .collect::<Vec<_>>();
    let moved = carry_moves(hunks, &mut kept, &edited)?;
    Some(Kept {
        text: apply(laid_out, hunks, &kept, newline),
        moved,
    })
}

/// Carry, with the hunks `kept` marks, the ones left out that put text the
/// formatter moved out of the edit, whose lines are `edited`, in its new place,
/// marking them kept too: whether any were carried, or `None` if text still
/// moves between the hunks kept and those left out ([`Words::moved_across`]).
///
/// A hunk left out is carried if it is *pure*, doing nothing but place the
/// edit's moved text ([`moved_text`]):
///
/// - it removes no line,
/// - it adds a word that crosses the boundary, and
/// - it adds no word more times than the moved text holds it, less what the
///   hunks carried before it placed.
///
/// A hunk this far from the edit can only remove a line that was already
/// there, so text moved into the edit from elsewhere is never carried, and
/// neither is a hunk that also restyles or reorders the lines where the edit's
/// text lands, or that adds words the edit didn't lose.
///
/// One pass carries all that can be carried. A pure hunk adds only words the
/// kept hunks remove on balance, and no more of a word than they do, so
/// carrying it can't start another word crossing; whatever still crosses after
/// the pass is in a hunk that isn't pure.
fn carry_moves(hunks: &[Hunk<'_, str>], kept: &mut [bool], edited: &Range<usize>) -> Option<bool> {
    let words = hunks.iter().map(Words::of).collect::<Vec<_>>();
    let (inside, outside) = sides(&words, kept);
    let crossing = inside.moved_across(&outside);
    if crossing.is_empty() {
        return Some(false);
    }
    let mut unplaced = moved_text(hunks, kept, &inside, edited);
    for (index, hunk) in hunks.iter().enumerate() {
        if kept[index] {
            continue;
        }
        // Placing its words spends them, so it is tried last.
        kept[index] = hunk.old_range().is_empty()
            && words[index].adds_any(&crossing)
            && unplaced.place(&words[index]);
    }
    let (inside, outside) = sides(&words, kept);
    inside.moved_across(&outside).is_empty().then_some(true)
}

/// The edit's moved text: word by word, what the kept hunks, whose words are
/// `inside`, remove on balance, as far as the lines of the edit they remove hold
/// it. What they remove from the line on either side of the edit was already
/// there, so it isn't the edit's to move.
fn moved_text(
    hunks: &[Hunk<'_, str>],
    kept: &[bool],
    inside: &Words,
    edited: &Range<usize>,
) -> Words {
    let mut removed = Words::default();
    for (hunk, _) in hunks.iter().zip(kept).filter(|(_, kept)| **kept) {
        let mut line = replaced_lines(hunk).start;
        for change in hunk.lines() {
            match change {
                Line::Delete(text) => {
                    if edited.contains(&line) {
                        removed.count(text, 1);
                    }
                    line += 1;
                }
                Line::Context(_) => line += 1,
                Line::Insert(_) => {}
            }
        }
    }
    removed.0.retain(|word, count| {
        *count = (*count).min(inside.get(word));
        *count > 0
    });
    removed
}

/// The words of the hunks `kept` marks, and those of the rest.
fn sides(words: &[Words], kept: &[bool]) -> (Words, Words) {
    let (mut inside, mut outside) = (Words::default(), Words::default());
    for (words, kept) in words.iter().zip(kept) {
        if *kept {
            inside.add(words);
        } else {
            outside.add(words);
        }
    }
    (inside, outside)
}

/// `laid_out` with the hunks `kept` marks applied, each line they write ending
/// with `newline`.
fn apply(laid_out: &str, hunks: &[Hunk<'_, str>], kept: &[bool], newline: &str) -> String {
    let lines = laid_out.split_inclusive('\n').collect::<Vec<_>>();
    let mut applied = String::with_capacity(laid_out.len());
    let mut copied = 0;
    for (hunk, _) in hunks.iter().zip(kept).filter(|(_, kept)| **kept) {
        let replaced = replaced_lines(hunk);
        for line in &lines[copied..replaced.start] {
            applied.push_str(line);
        }
        for line in hunk.lines() {
            if let Line::Insert(line) = line {
                push_line(&mut applied, line, newline);
            }
        }
        copied = replaced.end;
    }
    for line in &lines[copied..] {
        applied.push_str(line);
    }
    applied
}

/// The words some lines remove, less those they add: each word, without regard
/// to case, with how many more times it is removed than added, so a negative
/// count is a word added. A word is a run of letters, digits and underscores:
/// identifiers, keywords, numbers, and the words of strings and comments.
///
/// A formatter changes how code is laid out and punctuated, and almost never its
/// words, so within the hunks around an edit, the words removed are the words
/// added. Where they aren't, a word that some changes remove and others add was
/// moved from one to the other.
#[derive(Debug, Default)]
struct Words(HashMap<String, isize>);

impl Words {
    /// The words `hunk` removes, less those it adds.
    fn of(hunk: &Hunk<'_, str>) -> Self {
        let mut words = Self::default();
        for line in hunk.lines() {
            match line {
                Line::Delete(text) => words.count(text, 1),
                Line::Insert(text) => words.count(text, -1),
                Line::Context(_) => {}
            }
        }
        words
    }

    /// Count each word of `text` as removed `times` more times.
    fn count(&mut self, text: &str, times: isize) {
        let words = text
            .split(|c: char| !(c.is_alphanumeric() || c == '_'))
            .filter(|word| !word.is_empty());
        for word in words {
            *self.0.entry(word.to_lowercase()).or_default() += times;
        }
    }

    /// How many more times these remove `word` than add it.
    fn get(&self, word: &str) -> isize {
        self.0.get(word).copied().unwrap_or_default()
    }

    /// Count `other`'s words too.
    fn add(&mut self, other: &Self) {
        for (word, count) in &other.0 {
            *self.0.entry(word.clone()).or_default() += count;
        }
    }

    /// The words these remove on balance that `other` adds on balance, or the
    /// other way round: text moved between the two.
    fn moved_across(&self, other: &Self) -> HashSet<&str> {
        self.0
            .iter()
            .filter(|(word, count)| count.signum() * other.get(word).signum() < 0)
            .map(|(word, _)| word.as_str())
            .collect()
    }

    /// Whether these add one of `words` on balance.
    fn adds_any(&self, words: &HashSet<&str>) -> bool {
        words.iter().any(|word| self.get(word) < 0)
    }

    /// Taking these as words still to place, place those `other` adds, if
    /// these hold them all: whether they did. Nothing is placed if they don't.
    fn place(&mut self, other: &Self) -> bool {
        let added = || other.0.iter().filter(|(_, count)| **count < 0);
        if !added().all(|(word, count)| self.get(word) >= -count) {
            return false;
        }
        for (word, count) in added() {
            self.0.entry(word.clone()).and_modify(|left| *left += count);
        }
        true
    }
}

/// Whether a hunk that replaces the lines `replaced` touches the lines `near`. A
/// hunk that only inserts lines touches the line on each side of where they go.
fn touches(replaced: &Range<usize>, near: &Range<usize>) -> bool {
    if replaced.is_empty() {
        near.start <= replaced.start && replaced.start <= near.end
    } else {
        replaced.start < near.end && near.start < replaced.end
    }
}

/// The lines, indexes from 0, that `hunk` replaces. diffy numbers a hunk's lines
/// from 1, except that an empty range starts at the line it follows.
fn replaced_lines(hunk: &Hunk<'_, str>) -> Range<usize> {
    let range = hunk.old_range();
    let start = if range.is_empty() {
        range.start()
    } else {
        range.start() - 1
    };
    start..start + range.len()
}

/// Whether the edit's text on its first line is only whitespace that ends the
/// line, and the line reads as `source`'s did. Everything before `span` is as it
/// was, so that line of `source` starts where this one does.
fn seam_begins(source: &str, laid_out: &str, span: &Range<usize>) -> bool {
    let start = line_start(laid_out, span.start);
    let end = next_line(laid_out, span.start);
    is_blank(&laid_out[span.start..end]) && source[start..].starts_with(&laid_out[start..end])
}

/// Whether the edit's text on its last line is only whitespace that begins the
/// line, and the line reads as one of `source`'s did. Everything after `span` is
/// as it was, so that line of `source` ends as far from the end of the file as
/// this one does.
fn seam_ends(source: &str, laid_out: &str, span: &Range<usize>) -> bool {
    let start = line_start(laid_out, span.end - 1);
    let end = next_line(laid_out, span.end - 1);
    let after = laid_out.len() - end;
    let Some(source_start) = source.len().checked_sub(after + (end - start)) else {
        return false;
    };
    is_blank(&laid_out[start..span.end])
        && line_start(source, source_start) == source_start
        && source.get(source_start..source.len() - after) == Some(&laid_out[start..end])
}

/// The index, from 0, of the line `byte` is on.
fn line_index(text: &str, byte: usize) -> usize {
    text.as_bytes()[..byte]
        .iter()
        .filter(|b| **b == b'\n')
        .count()
}

/// The start of the line `byte` is on.
fn line_start(text: &str, byte: usize) -> usize {
    text.as_bytes()[..byte]
        .iter()
        .rposition(|b| *b == b'\n')
        .map_or(0, |newline| newline + 1)
}

/// The start of the line after the one `byte` is on, or the end of `text`.
fn next_line(text: &str, byte: usize) -> usize {
    text.as_bytes()[byte..]
        .iter()
        .position(|b| *b == b'\n')
        .map_or(text.len(), |newline| byte + newline + 1)
}

fn is_blank(text: &str) -> bool {
    text.chars().all(char::is_whitespace)
}

/// `text` with each CRLF line ending as LF.
fn lf(text: &str) -> Cow<'_, str> {
    if text.contains("\r\n") {
        Cow::Owned(text.replace("\r\n", "\n"))
    } else {
        Cow::Borrowed(text)
    }
}

/// Push `line`, ending with `newline` if it ends at all.
fn push_line(text: &mut String, line: &str, newline: &str) {
    match line.strip_suffix('\n') {
        Some(line) => {
            text.push_str(line);
            text.push_str(newline);
        }
        None => text.push_str(line),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `source` with `text` in place of `source[start..end]`, and the span of
    /// `text` in the result.
    fn splice(source: &str, start: usize, end: usize, text: &str) -> (String, Range<usize>) {
        let laid_out = [&source[..start], text, &source[end..]].concat();
        (laid_out, start..start + text.len())
    }

    /// The lines `edited_lines` finds for replacing `old` in `source` with `new`.
    fn edited(source: &str, old: &str, new: &str) -> Range<usize> {
        let start = source.find(old).unwrap();
        let (laid_out, span) = splice(source, start, start + old.len(), new);
        edited_lines(source, &laid_out, span)
    }

    /// `n` numbered lines, `line 0` to `line {n - 1}`.
    fn numbered(n: usize) -> String {
        (0..n).map(|index| format!("line {index}\n")).collect()
    }

    /// What [`keep`] keeps, when it keeps anything.
    fn kept(laid_out: &str, formatted: &str, edited: Range<usize>, newline: &str) -> String {
        keep(laid_out, formatted, edited, newline)
            .expect("nothing moved across the boundary")
            .text
    }

    /// What [`keep`] keeps, having carried a move to where the formatter put it.
    fn carried(text: impl Into<String>) -> Option<Kept> {
        Some(Kept {
            text: text.into(),
            moved: true,
        })
    }

    /// `text` with each line named in `lines` given the `suffix`, as a formatter
    /// would change it.
    fn changed(text: &str, lines: &[usize], suffix: &str) -> String {
        text.split_inclusive('\n')
            .enumerate()
            .map(|(index, line)| match lines.contains(&index) {
                true => format!("{}{suffix}\n", line.trim_end_matches('\n')),
                false => line.to_owned(),
            })
            .collect()
    }

    #[test]
    fn a_replacement_on_one_line_is_that_line() {
        let source = "fn a() {\n    let x = 1;\n}\n";
        assert_eq!(edited(source, "let x = 1;", "let x=2;"), 1..2);
    }

    #[test]
    fn a_multi_line_replacement_is_every_line_it_is_on() {
        let source = "fn a() {\n    let x = 1;\n}\n\nfn b() {}\n";
        // Even the lines that read as they did: the content is the edit.
        assert_eq!(
            edited(
                source,
                "fn a() {\n    let x = 1;\n}",
                "fn a() {\n    let x=2;\n}"
            ),
            0..3
        );
    }

    /// The line break that ends the line an insert goes below is the insert's
    /// own, but the line reads as it did, so it isn't edited.
    #[test]
    fn an_insert_below_a_line_is_its_own_lines() {
        let source = "fn a() {\n    let x = 1;\n}\n";
        let at = source.find("let x = 1;").unwrap() + "let x = 1;".len();
        let (laid_out, span) = splice(source, at, at, "\n    let y=2;");
        assert_eq!(edited_lines(source, &laid_out, span), 2..3);

        // So are the blank lines an insert asks for.
        let (laid_out, span) = splice(source, at, at, "\n\n    let y=2;");
        assert_eq!(edited_lines(source, &laid_out, span), 2..4);
    }

    /// The indentation given back to the line an insert goes above is the
    /// insert's own, but the line reads as it did, so it isn't edited.
    #[test]
    fn an_insert_above_a_line_is_its_own_lines() {
        let source = "fn a() {\n    let x = 1;\n}\n";
        let at = source.find("let x = 1;").unwrap();
        let (laid_out, span) = splice(source, at, at, "let w=0;\n    ");
        assert_eq!(laid_out, "fn a() {\n    let w=0;\n    let x = 1;\n}\n");
        assert_eq!(edited_lines(source, &laid_out, span), 1..2);
    }

    /// Text that only supplies a line, a blank one, is still on a line.
    #[test]
    fn an_insert_of_a_line_break_is_a_line() {
        let source = "a\nb\n";
        let (laid_out, span) = splice(source, 1, 1, "\n");
        assert_eq!(edited_lines(source, &laid_out, span), 0..1);
    }

    /// A deletion of whole lines is the point they went from: between the lines
    /// on either side of it.
    #[test]
    fn a_deletion_of_whole_lines_is_a_point_between_two() {
        let source = "a\nb\nc\nd\n";
        let (laid_out, span) = splice(source, 2, 6, "");
        assert_eq!(laid_out, "a\nd\n");
        assert_eq!(edited_lines(source, &laid_out, span), 1..1);

        // At either end of the file too.
        let (laid_out, span) = splice(source, 0, 2, "");
        assert_eq!(edited_lines(source, &laid_out, span), 0..0);
        let (laid_out, span) = splice(source, 6, 8, "");
        assert_eq!(edited_lines(source, &laid_out, span), 3..3);
    }

    #[test]
    fn a_deletion_inside_a_line_is_that_line() {
        assert_eq!(edited("f(a, b, c);\nnext\n", ", b", ""), 0..1);
    }

    #[test]
    fn crlf_lines_are_found_alike() {
        let source = "fn a() {\r\n    let x = 1;\r\n}\r\n";
        let at = source.find("let x = 1;").unwrap() + "let x = 1;".len();
        let (laid_out, span) = splice(source, at, at, "\r\n    let y=2;");
        assert_eq!(edited_lines(source, &laid_out, span), 2..3);

        let at = source.find("let x = 1;").unwrap();
        let (laid_out, span) = splice(source, at, at, "let w=0;\r\n    ");
        assert_eq!(edited_lines(source, &laid_out, span), 1..2);
    }

    /// A change to an edited line is kept, as is one to the line on either side
    /// of the edit; one further away is left out.
    #[test]
    fn changes_touching_the_edit_or_next_to_it_are_kept_and_the_rest_left_out() {
        let laid_out = numbered(12);
        for (line, keeps) in [
            (3, false),
            (4, true),
            (5, true),
            (6, true),
            (7, true),
            (8, false),
        ] {
            let formatted = changed(&laid_out, &[line], " // formatted");
            let expected = if keeps { &formatted } else { &laid_out };
            assert_eq!(
                &kept(&laid_out, &formatted, 5..7, "\n"),
                expected,
                "line {line}"
            );
        }
    }

    #[test]
    fn several_changes_can_be_kept_together() {
        let laid_out = numbered(12);
        let formatted = changed(&laid_out, &[0, 4, 6, 7, 11], " // formatted");
        assert_eq!(
            kept(&laid_out, &formatted, 5..7, "\n"),
            changed(&laid_out, &[4, 6, 7], " // formatted")
        );
    }

    /// The formatter's hunk is applied whole, even where it reaches beyond the
    /// line before the edit.
    #[test]
    fn a_change_straddling_the_boundary_is_kept_whole() {
        let laid_out = numbered(12);
        let formatted = changed(&laid_out, &[1, 2, 3, 4], " // formatted");
        assert_eq!(kept(&laid_out, &formatted, 5..7, "\n"), formatted);

        // One that only reaches the line before the line before is left out.
        let formatted = changed(&laid_out, &[1, 2, 3], " // formatted");
        assert_eq!(kept(&laid_out, &formatted, 5..7, "\n"), laid_out);
    }

    /// Lines the formatter inserts touch the line on either side of them, and
    /// lines it removes are the lines they were.
    #[test]
    fn insertions_and_removals_are_kept_by_where_they_are() {
        let laid_out = "a\nb\nedit\nc\nd\n";
        for (formatted, keeps) in [
            ("a\n\nb\nedit\nc\nd\n", true),
            ("\na\nb\nedit\nc\nd\n", false),
            ("a\nb\nedit\nc\n\nd\n", true),
            ("a\nb\nedit\nc\nd\n\n", false),
            ("a\nedit\nc\nd\n", true),
            ("b\nedit\nc\nd\n", false),
        ] {
            let expected = if keeps { formatted } else { laid_out };
            assert_eq!(
                kept(laid_out, formatted, 2..3, "\n"),
                expected,
                "{formatted:?}"
            );
        }
    }

    /// A deletion is the point where the text was removed, which touches the
    /// line on either side of it.
    #[test]
    fn a_deletions_neighbors_are_formatted() {
        let laid_out = numbered(8);
        for (line, keeps) in [(2, false), (3, true), (4, true), (5, false)] {
            let formatted = changed(&laid_out, &[line], " // formatted");
            let expected = if keeps { &formatted } else { &laid_out };
            assert_eq!(
                &kept(&laid_out, &formatted, 4..4, "\n"),
                expected,
                "line {line}"
            );
        }
        // Lines inserted where the text was removed are kept.
        let formatted = laid_out.replacen("line 4\n", "\nline 4\n", 1);
        assert_eq!(kept(&laid_out, &formatted, 4..4, "\n"), formatted);
    }

    /// rustfmt and biome write LF whatever the file uses: the lines they leave
    /// as they were aren't changes, and the lines a kept change writes take the
    /// file's line endings again.
    #[test]
    fn a_crlf_files_kept_lines_end_as_its_others_do() {
        let laid_out =
            "fn a() {\r\n        let x = 1;\r\n}\r\n\r\nfn b() {\r\n    let y=2;\r\n}\r\n";
        let formatted = "fn a() {\n    let x = 1;\n}\n\nfn b() {\n    let y = 2;\n}\n";
        assert_eq!(
            kept(laid_out, formatted, 5..6, "\r\n"),
            "fn a() {\r\n        let x = 1;\r\n}\r\n\r\nfn b() {\r\n    let y = 2;\r\n}\r\n"
        );
        // ruff keeps CRLF, which reads the same.
        let crlf = formatted.replace('\n', "\r\n");
        assert_eq!(
            kept(laid_out, &crlf, 5..6, "\r\n"),
            kept(laid_out, formatted, 5..6, "\r\n")
        );
    }

    /// A last line without a line break keeps going without one, and gets one
    /// only when the formatter's change to it is kept.
    #[test]
    fn the_last_line_keeps_or_gets_its_line_break_with_the_change() {
        let laid_out = "a\nb\nc";
        let formatted = "a\nb\nc\n";
        assert_eq!(kept(laid_out, formatted, 2..3, "\n"), formatted);
        assert_eq!(kept(laid_out, formatted, 0..1, "\n"), laid_out);
    }

    #[test]
    fn no_change_keeps_the_file_as_laid_out() {
        let laid_out = numbered(5);
        assert_eq!(kept(&laid_out, &laid_out, 2..3, "\n"), laid_out);
        assert_eq!(kept("", "", 0..0, "\n"), "");
    }

    /// rustfmt sorts imports: a line diff sees the import added at the end
    /// removed there and added far above. Keeping only the removal would lose
    /// the edit's import from a file that still parses, so the addition, which
    /// does nothing else, is kept too.
    #[test]
    fn a_line_moved_out_of_the_edit_goes_where_the_formatter_put_it() {
        let laid_out = "use a::A;\nuse b::B;\nuse c::C;\nuse d::D;\nuse e::E;\nuse aa::AA;\n";
        let formatted = "use a::A;\nuse aa::AA;\nuse b::B;\nuse c::C;\nuse d::D;\nuse e::E;\n";
        assert_eq!(keep(laid_out, formatted, 5..6, "\n"), carried(formatted));
        // Moved with a change, as when the formatter sorts inside its braces too.
        let formatted = formatted.replace("aa::AA", "aa::{X, Y}");
        let laid_out = laid_out.replace("aa::AA", "aa::{Y,X}");
        assert_eq!(keep(&laid_out, &formatted, 5..6, "\n"), carried(&formatted));
        // In a CRLF file, the line takes the file's line ending where it lands.
        let crlf = laid_out.replace('\n', "\r\n");
        assert_eq!(
            keep(&crlf, &formatted, 5..6, "\r\n"),
            carried(formatted.replace('\n', "\r\n"))
        );
    }

    /// The formatter's changes far from the edit that don't place its text stay
    /// left out, even where they move lines of their own.
    #[test]
    fn a_move_is_carried_without_the_changes_around_it() {
        let laid_out = "use std::io;\nuse std::fmt;\n\nuse a::A;\nuse b::B;\nuse c::C;\nuse d::D;\nuse e::E;\nuse aa::AA;\n";
        let formatted = "use std::fmt;\nuse std::io;\n\nuse a::A;\nuse aa::AA;\nuse b::B;\nuse c::C;\nuse d::D;\nuse e::E;\n";
        assert_eq!(
            keep(laid_out, formatted, 8..9, "\n"),
            carried(
                "use std::io;\nuse std::fmt;\n\nuse a::A;\nuse aa::AA;\nuse b::B;\nuse c::C;\nuse d::D;\nuse e::E;\n"
            )
        );
    }

    /// Lines of the edit that land in different places are each carried there.
    #[test]
    fn a_move_to_several_places_is_carried_to_each() {
        let laid_out = "use a::A;\nuse b::B;\nuse c::C;\nuse d::D;\nuse e::E;\nuse f::F;\nuse aa::AA;\nuse cc::CC;\n";
        let formatted = "use a::A;\nuse aa::AA;\nuse b::B;\nuse c::C;\nuse cc::CC;\nuse d::D;\nuse e::E;\nuse f::F;\n";
        assert_eq!(keep(laid_out, formatted, 6..8, "\n"), carried(formatted));
    }

    /// A line moved from far away to the edit would be in the file twice, and
    /// removing it from where it was would change a line already there.
    #[test]
    fn a_line_moved_into_the_edit_keeps_nothing() {
        let laid_out = "use z::Z;\nuse b::B;\nuse c::C;\nuse d::D;\nuse e::E;\nuse f::F;\n";
        let formatted = "use b::B;\nuse c::C;\nuse d::D;\nuse e::E;\nuse f::F;\nuse z::Z;\n";
        assert_eq!(keep(laid_out, formatted, 5..6, "\n"), None);
    }

    /// Where the edit's text lands, the formatter also restyles a line that was
    /// already there: carrying the change would restyle it too.
    #[test]
    fn a_move_into_other_changes_keeps_nothing() {
        let laid_out = "use a::A;\nuse  b::B;\nuse c::C;\nuse d::D;\nuse e::E;\nuse aa::AA;\n";
        let formatted = "use a::A;\nuse aa::AA;\nuse b::B;\nuse c::C;\nuse d::D;\nuse e::E;\n";
        assert_eq!(keep(laid_out, formatted, 5..6, "\n"), None);
        // Or reorders lines that were already there.
        let laid_out = "use a::A;\nuse c::C;\nuse b::B;\nuse d::D;\nuse e::E;\nuse aa::AA;\n";
        assert_eq!(keep(laid_out, formatted, 5..6, "\n"), None);
    }

    /// A change that adds more than the edit's moved text isn't only placing
    /// it.
    #[test]
    fn a_move_that_adds_other_words_keeps_nothing() {
        let laid_out = "use a::A;\nuse b::B;\nuse c::C;\nuse d::D;\nuse e::E;\nuse aa::AA;\n";
        let formatted =
            "use a::A;\nuse aa::AA;\nuse ab::AB;\nuse b::B;\nuse c::C;\nuse d::D;\nuse e::E;\n";
        assert_eq!(keep(laid_out, formatted, 5..6, "\n"), None);
        // Nor is one that adds the edit's line twice.
        let formatted = formatted.replace("ab::AB", "aa::AA");
        assert_eq!(keep(laid_out, &formatted, 5..6, "\n"), None);
    }

    /// The line before the edit was out of order before it: the formatter
    /// moving it is a change to a line already there, not to the edit.
    #[test]
    fn a_line_moved_from_beside_the_edit_keeps_nothing() {
        let laid_out = "use a::A;\nuse b::B;\nuse c::C;\nuse d::D;\nuse aa::AA;\nuse f::F;\n";
        let formatted = "use a::A;\nuse aa::AA;\nuse b::B;\nuse c::C;\nuse d::D;\nuse f::F;\n";
        assert_eq!(keep(laid_out, formatted, 5..6, "\n"), None);
    }

    /// A move whose two ends are both kept is a change like any other.
    #[test]
    fn a_line_moved_within_what_is_kept_is_kept() {
        let laid_out = "use a::A;\nuse b::B;\nuse d::D;\nuse c::C;\nuse e::E;\n";
        let formatted = "use a::A;\nuse b::B;\nuse c::C;\nuse d::D;\nuse e::E;\n";
        assert_eq!(
            keep(laid_out, formatted, 3..4, "\n"),
            Some(Kept {
                text: formatted.into(),
                moved: false
            })
        );
    }

    /// Words that only change case or punctuation, as numbers and quotes do,
    /// haven't moved, and neither has a word the formatter adds to the edit
    /// alone.
    #[test]
    fn restyled_words_havent_moved() {
        let laid_out = "x = 0XFF\ny = 'a'\nz = 1\nw = 2\nv = 0XAB\n";
        let formatted = "x = 0xFF\ny = 'a'\nz = 1\nw = 2\nv = 0xAB\n";
        assert_eq!(
            kept(laid_out, formatted, 0..1, "\n"),
            "x = 0xFF\ny = 'a'\nz = 1\nw = 2\nv = 0XAB\n"
        );
        let laid_out = "extern fn f();\n\n\n\nfn g() {}\n";
        let formatted = "extern \"C\" fn f();\n\n\n\nfn g() {}\n";
        assert_eq!(kept(laid_out, formatted, 0..1, "\n"), formatted);
    }
}
