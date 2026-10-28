//! Formatting the winning candidate. The formatter may veto an edit, but it
//! never moves one: validation alone picks the candidate, and the formatter
//! runs once, on that candidate's result (`docs/design/formatting.md`,
//! "Formatting").
//!
//! The formatter formats the whole result, but unless the settings ask for the
//! [whole file](crate::formatting::Formatting::whole_file), only its changes to
//! the edit are kept ([`hunks`]), and the result is validated again. Text the
//! formatter moves out of the edit, as sorting imports does, goes where the
//! formatter put it, if nothing else changes there. Changes that can't stand
//! without the rest of the formatter's, because the file isn't valid without
//! them or because the formatter moved text between them and the rest in any
//! other way, are dropped with the rest: the edit is written as laid out, with
//! a note. The edit itself validated, so it isn't rejected, and writing the
//! whole formatted file instead would change lines far from it.
//!
//! A formatter's failure is judged per file:
//!
//! | The formatter | Treated as |
//! |---|---|
//! | can't run (not found, not executable, timed out) | absent: the result is written as laid out, with a note |
//! | fails on the file as it was before the edit | absent for this edit, with a note quoting its error |
//! | succeeds before the edit and fails after | caused by the edit, which is rejected |
//! | succeeds, but its output fails validation | a failure, judged by the same rule |
//!
//! The file before the edit is formatted only when formatting the edit fails.
//! A source that already had syntax errors is being repaired, and formatters
//! refuse broken code, so there a failure leaves the repair unformatted with a
//! warning, as it always has.

mod hunks;

use super::Editor;
use crate::{
    formatting::{Failure, Formatter},
    layout::line_ending,
};
use std::ops::Range;

/// The longest a formatter's error gets in a one-line note.
const SUMMARY_LIMIT: usize = 200;

/// What happened to an applied edit's whitespace, which its report says first.
pub(crate) enum Whitespace {
    /// Written as laid out: the file has no formatter, or the result is a repair
    /// that still has syntax errors, which formatters refuse.
    AsLaidOut,
    /// The file's formatter formatted the edit, and only its changes to the edit
    /// were kept. `moved` if it moved some of the edit's text elsewhere in the
    /// file, and that text was written where it moved it.
    FormattedEdit { formatter: Formatter, moved: bool },
    /// The file's formatter formatted the whole file, and all of its changes
    /// were kept.
    FormattedFile(Formatter),
    /// Written as laid out because the formatter couldn't be used on this file:
    /// the note saying why, a line of its own.
    Unformatted(String),
    /// A repair the formatter failed on, written as laid out: the warning,
    /// which quotes the formatter's error.
    RepairUnformatted(String),
}

/// The winning candidate's result as it is written, or the formatter's veto.
pub(crate) enum Formatted {
    Written {
        output: String,
        whitespace: Whitespace,
    },
    /// The edit is rejected: the report saying why.
    Vetoed(String),
}

/// Why formatting a source gave nothing to write.
#[derive(Debug, Clone, PartialEq, Eq)]
enum Refusal {
    /// The formatter couldn't run: why, as a phrase.
    Unavailable(String),
    /// It failed: its error.
    Failed(String),
    /// It succeeded, but its output fails validation: what validation found.
    Invalid(String),
}

impl From<Failure> for Refusal {
    fn from(failure: Failure) -> Self {
        match failure {
            Failure::Unavailable(reason) => Self::Unavailable(reason),
            Failure::Failed(error) => Self::Failed(error),
        }
    }
}

impl Refusal {
    /// The refusal in full, for a rejection.
    fn error(&self) -> String {
        match self {
            Self::Unavailable(reason) => reason.clone(),
            Self::Failed(error) => error.clone(),
            Self::Invalid(errors) => format!("Its output is invalid:\n\n{errors}"),
        }
    }

    /// The refusal in a phrase, for a note.
    fn summary(&self) -> String {
        match self {
            Self::Unavailable(reason) => reason.clone(),
            Self::Failed(error) => summary(error),
            Self::Invalid(_) => String::from("its output is invalid"),
        }
    }
}

impl Editor<'_> {
    /// `output`, the winning candidate's result, as it is written: formatted by
    /// the file's formatter, if it has one and can format it, or vetoed by it
    /// (see the module docs). `span` is the byte range of `output` that holds the
    /// edit's text, which is empty for a deletion.
    pub(super) fn format(&self, output: String, span: Range<usize>) -> Formatted {
        let Some(formatter) = &self.formatter else {
            return written(output, Whitespace::AsLaidOut);
        };
        let repairing = self.starts_with_syntax_errors();
        if repairing && self.parsed_syntax_errors(&output) {
            return written(output, Whitespace::AsLaidOut);
        }

        let refusal = match self.formatted(formatter, &output) {
            Ok(formatted) => return self.scoped(formatter, output, formatted, span),
            Err(refusal) => refusal,
        };
        let whitespace = match refusal {
            Refusal::Unavailable(reason) => Whitespace::Unformatted(cant_run(formatter, &reason)),
            refusal if repairing => Whitespace::RepairUnformatted(format!(
                "Warning: formatting failed; the edit was accepted without formatting because \
                 the source already had syntax errors.\n\n{}",
                refusal.error()
            )),
            refusal => match self.formatted(formatter, &self.source_code) {
                // The file formats without the edit, so the edit is what fails.
                Ok(_) => {
                    return Formatted::Vetoed(format!(
                        "The formatter has encountered the following error making that change, \
                         so the file has not been modified. The tool has prevented what it \
                         believes to be an unsafe edit. Please try a different edit.\n\n{}\n\n{}",
                        refusal.error(),
                        self.diff(&output)
                    ));
                }
                Err(Refusal::Unavailable(reason)) => {
                    Whitespace::Unformatted(cant_run(formatter, &reason))
                }
                Err(before) => Whitespace::Unformatted(format!(
                    "Note: {formatter} fails on this file even without the edit ({}), so this \
                     edit wasn't formatted; the diff shows exactly what was written.",
                    before.summary()
                )),
            },
        };
        written(output, whitespace)
    }

    /// What is written for `output`, whose edit's text is at `span`, given
    /// `formatted`, the formatter's valid output for it: all of it if the
    /// settings ask for the whole file. Otherwise `output` with the formatter's
    /// changes to the edit, if that is valid too, and else `output` as laid out,
    /// with a note.
    fn scoped(
        &self,
        formatter: &Formatter,
        output: String,
        formatted: String,
        span: Range<usize>,
    ) -> Formatted {
        if self.whole_file {
            return written(formatted, Whitespace::FormattedFile(formatter.clone()));
        }
        let edited = hunks::edited_lines(&self.source_code, &output, span);
        let newline = line_ending(&self.source_code);
        let formatted_edit = |moved| Whitespace::FormattedEdit {
            formatter: formatter.clone(),
            moved,
        };
        let refusal = match hunks::keep(&output, &formatted, edited, newline) {
            // All of the formatter's changes, or none of them, is a result
            // already validated.
            Some(kept) if kept.text == formatted || kept.text == output => {
                return written(kept.text, formatted_edit(kept.moved));
            }
            Some(kept) => match self.invalid(&kept.text) {
                None => return written(kept.text, formatted_edit(kept.moved)),
                Some(errors) => errors,
            },
            None => String::from(
                "they move text between the edit and the rest of the file, and not only the \
                 edit's own text to a place where nothing else changes",
            ),
        };
        log::debug!("{formatter}'s changes to the edit alone are refused:\n{refusal}");
        written(
            output,
            Whitespace::Unformatted(format!(
                "Note: {formatter}'s changes couldn't be applied to the edit alone, so this edit \
                 wasn't formatted; the diff shows exactly what was written."
            )),
        )
    }

    /// `source` formatted by `formatter`, if it formats and its output passes
    /// the validation an edit's result must pass.
    fn formatted(&self, formatter: &Formatter, source: &str) -> Result<String, Refusal> {
        let formatted = formatter.format(source, &self.file_path, &self.source_code)?;
        match self.invalid(&formatted) {
            Some(errors) => Err(Refusal::Invalid(errors)),
            None => Ok(formatted),
        }
    }

    /// What fails the validation an edit's result must pass in `result`, a
    /// formatted one, if anything does.
    fn invalid(&self, result: &str) -> Option<String> {
        let Some(tree) = self.parse(result, None) else {
            return Some(String::from("It couldn't be parsed."));
        };
        // A repair is accepted unvalidated, but a formatter mustn't break the
        // syntax the repair restored.
        if self.starts_with_syntax_errors() {
            Self::syntax_errors(self.language, &tree, result)
        } else {
            self.validate_tree(&tree, result)
                .map(|failure| failure.message)
        }
    }

    /// Whether `output` has syntax errors, or doesn't parse at all.
    fn parsed_syntax_errors(&self, output: &str) -> bool {
        self.parse(output, None)
            .is_none_or(|tree| Self::syntax_errors(self.language, &tree, output).is_some())
    }
}

fn written(output: String, whitespace: Whitespace) -> Formatted {
    Formatted::Written { output, whitespace }
}

/// The note for a formatter that couldn't run.
fn cant_run(formatter: &Formatter, reason: &str) -> String {
    format!(
        "Note: {formatter} couldn't run ({reason}), so this edit wasn't formatted; the diff \
         shows exactly what was written."
    )
}

/// The line of a formatter's error that says what's wrong, shortened to fit a
/// note: a `×`-marked diagnostic if there is one, as biome's message is under
/// its header line, or else the first line with anything on it.
fn summary(error: &str) -> String {
    let mut lines = error.lines().map(str::trim).filter(|line| !line.is_empty());
    let marked = lines
        .clone()
        .find_map(|line| line.strip_prefix('×'))
        .map(str::trim_start);
    let line = marked.or_else(|| lines.next()).unwrap_or("it failed");
    match line.char_indices().nth(SUMMARY_LIMIT) {
        Some((end, _)) => format!("{}…", line[..end].trim_end()),
        None => line.to_owned(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_summary_is_the_first_line_with_anything_on_it() {
        assert_eq!(
            summary(
                "error: expected expression, found `;`\n --> <stdin>:2:13\n  |\n2 |     let x = ;\n"
            ),
            "error: expected expression, found `;`"
        );
        assert_eq!(summary("\n\n   oops  \n"), "oops");
        assert_eq!(summary(""), "it failed");
    }

    #[test]
    fn a_summary_prefers_a_marked_diagnostic_to_its_header() {
        let biome = "format ━━━━━━━━━━━━━━━━━━━━\n\n  × Code formatting aborted due to parsing \
                     errors. To format code with errors, enable the 'formatter.formatWithErrors' \
                     option.\n  \n";
        assert_eq!(
            summary(biome),
            "Code formatting aborted due to parsing errors. To format code with errors, enable \
             the 'formatter.formatWithErrors' option."
        );
    }

    #[test]
    fn a_long_summary_is_shortened_on_a_character_boundary() {
        let long = "é".repeat(SUMMARY_LIMIT + 50);
        let shortened = summary(&long);
        assert_eq!(shortened.chars().count(), SUMMARY_LIMIT + 1);
        assert!(shortened.ends_with('…'), "{shortened}");
        assert_eq!(
            summary(&"é".repeat(SUMMARY_LIMIT)),
            "é".repeat(SUMMARY_LIMIT)
        );
    }

    #[test]
    fn refusals_read_in_full_and_in_a_phrase() {
        let invalid = Refusal::Invalid(String::from("===SYNTAX ERRORS===\n   1 ->⎸fn ("));
        assert_eq!(
            invalid.error(),
            "Its output is invalid:\n\n===SYNTAX ERRORS===\n   1 ->⎸fn ("
        );
        assert_eq!(invalid.summary(), "its output is invalid");
        let failed = Refusal::Failed(String::from("error: bad\ndetails"));
        assert_eq!(failed.error(), "error: bad\ndetails");
        assert_eq!(failed.summary(), "error: bad");
    }
}
