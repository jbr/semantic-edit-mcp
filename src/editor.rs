mod edit;
mod edit_iterator;
mod edit_position;
mod format;

use crate::{
    formatting::{Formatter, Formatting, Scope},
    languages::{LanguageCommon, LanguageName},
    searcher::find_positions,
    selector::{Operation, Selector},
    state::{AppliedEdit, EditOperation},
    validation::{ContextValidator, Diagnostic, format_violations},
};
use anyhow::{Result, anyhow};
use diffy::{DiffOptions, Line as DiffyLine, PatchFormatter};
use ropey::Rope;
use std::{
    cell::OnceCell,
    collections::{BTreeMap, BTreeSet},
    iter,
    path::PathBuf,
};
use tree_sitter::Tree;

pub(crate) use edit::Edit;
pub(crate) use edit_iterator::EditIterator;
pub(crate) use edit_position::EditPosition;
use format::{Formatted, Whitespace};

/// What trying an edit's candidates came to.
enum Outcome {
    /// The winning candidate's result, as it is to be written.
    Applied {
        output: String,
        whitespace: Whitespace,
        /// Set when the winning candidate's anchor also matched elsewhere.
        ambiguity_note: Option<String>,
    },
    /// No candidate could be written: the report saying why.
    Rejected(String),
}

/// Why a candidate's result tree was rejected. `syntax: true` means the result
/// didn't even parse cleanly (the placement itself is structural nonsense);
/// `syntax: false` means it parsed but introduced a context-query violation or
/// failed the language's [check](crate::languages::traits::LanguageEditor::check).
pub(crate) struct ValidationFailure {
    pub(crate) message: String,
    pub(crate) syntax: bool,
}

#[derive(fieldwork::Fieldwork)]
#[fieldwork(get)]
pub struct Editor<'language> {
    content: String,
    selector: Selector,
    file_path: PathBuf,
    language: &'language LanguageCommon,
    /// The file's formatter under the calling tool's settings, if it has one.
    formatter: Option<Formatter>,
    /// How much of the file the formatter may change.
    #[field(copy)]
    scope: Scope,
    source_code: String,
    tree: Tree,
    rope: Rope,
    /// Whether the source passes its language's check, which gates candidates
    /// only if it does. Computed on first use, once for every candidate.
    #[field(skip)]
    passes_check: OnceCell<bool>,
}

impl<'language> Editor<'language> {
    /// An editor for the file at `file_path`, formatted as `formatting` says for
    /// `language`.
    pub fn new(
        content: String,
        selector: Selector,
        language: &'language LanguageCommon,
        formatting: &Formatting,
        file_path: PathBuf,
    ) -> Result<Self> {
        let source_code = std::fs::read_to_string(&file_path)?;
        Self::from_source(
            content,
            selector,
            language,
            formatting,
            file_path,
            source_code,
        )
    }

    /// Construct against an explicit source text instead of reading
    /// `file_path` from disk. Retargeting uses this to re-run an edit against
    /// the recorded *pre-edit* source while the file on disk already contains
    /// the edit being moved.
    pub fn from_source(
        content: String,
        selector: Selector,
        language: &'language LanguageCommon,
        formatting: &Formatting,
        file_path: PathBuf,
        source_code: String,
    ) -> Result<Self> {
        let mut parser = language.tree_sitter_parser()?;
        let tree = parser.parse(&source_code, None).ok_or_else(|| {
            anyhow!(
                "Unable to parse {} as {}",
                file_path.display(),
                language.name()
            )
        })?;
        let rope = Rope::from_str(&source_code);

        Ok(Self {
            content,
            selector,
            language,
            formatter: formatting.formatter(language.name()).cloned(),
            scope: formatting.scope(),
            tree,
            file_path,
            source_code,
            rope,
            passes_check: OnceCell::new(),
        })
    }

    /// A broken source is editable without a validity gate until it parses
    /// cleanly again. The next call observes that new baseline and restores
    /// the normal guard against making valid code invalid.
    fn starts_with_syntax_errors(&self) -> bool {
        !self
            .language
            .editor()
            .collect_errors(&self.tree, &self.source_code)
            .is_empty()
    }

    /// Validate a candidate result tree, relative to the file being edited: it
    /// must parse cleanly, introduce no context-query violation, and pass the
    /// language's [check](crate::languages::traits::LanguageEditor::check) if the
    /// file did.
    ///
    /// Unlike the absolute [`validate`](Self::validate), context-query violations
    /// and the check are compared against the *original* source: a violation
    /// that already exists in the unedited file is pre-existing code, not
    /// something this edit introduced, so it never fails the candidate. Without
    /// this, one imperfect heuristic match anywhere in a file makes the whole
    /// file uneditable — and the resulting error points at code the caller never
    /// touched.
    fn validate_tree(&self, tree: &Tree, content: &str) -> Option<ValidationFailure> {
        if self.starts_with_syntax_errors() {
            return None;
        }
        if let Some(errors) = Self::syntax_errors(self.language, tree, content) {
            return Some(ValidationFailure {
                message: errors,
                syntax: true,
            });
        }
        self.introduced_violations(tree, content)
            .or_else(|| self.check_failure(tree, content))
    }

    /// The context-query violations in a candidate's result that the original
    /// source doesn't already have.
    fn introduced_violations(&self, tree: &Tree, content: &str) -> Option<ValidationFailure> {
        let query = self.language.validation_query()?;
        let result = ContextValidator::validate_tree(tree, query, content);
        if result.is_valid {
            return None;
        }

        let baseline = ContextValidator::validate_tree(&self.tree, query, &self.source_code);
        let mut preexisting = BTreeMap::new();
        for violation in &baseline.violations {
            *preexisting.entry(violation.message.as_str()).or_insert(0) += 1;
        }
        let introduced = result
            .violations
            .iter()
            .filter(
                |violation| match preexisting.get_mut(violation.message.as_str()) {
                    Some(count) if *count > 0 => {
                        *count -= 1;
                        false
                    }
                    _ => true,
                },
            )
            .collect::<Vec<_>>();
        if introduced.is_empty() {
            return None;
        }

        Some(ValidationFailure {
            message: format_violations(introduced.into_iter(), content),
            syntax: false,
        })
    }

    /// The language's check of a candidate's result, if the original source
    /// passes it. A file the checker can't handle, with syntax newer than the
    /// checker or a nightly feature, is still editable; what remains unguarded
    /// is an edit introducing such syntax into a file that didn't use it.
    ///
    /// A result that fails the check parsed cleanly, so the placement was
    /// plausible and its failure explains what blocked it: it is reported over
    /// candidates that didn't parse, as a formatter's refusal was.
    fn check_failure(&self, tree: &Tree, content: &str) -> Option<ValidationFailure> {
        if !self.passes_check() {
            return None;
        }
        let diagnostic = self.language.editor().check(tree, content).err()?;
        Some(ValidationFailure {
            message: check_errors(&diagnostic, content),
            syntax: false,
        })
    }

    /// Whether the source as it was before the edit passes its language's check.
    fn passes_check(&self) -> bool {
        *self.passes_check.get_or_init(|| {
            self.language
                .editor()
                .check(&self.tree, &self.source_code)
                .is_ok()
        })
    }

    /// Absolute validation of a standalone tree — syntax plus *all* context-query
    /// violations, with no baseline to compare against. Candidate edits go
    /// through [`validate_tree`](Self::validate_tree) instead, which only fails
    /// on violations the edit introduced; this absolute form remains the
    /// semantic-validation test surface.
    #[cfg_attr(not(test), allow(dead_code))]
    pub fn validate(language: &LanguageCommon, tree: &Tree, content: &str) -> Option<String> {
        if let Some(errors) = Self::syntax_errors(language, tree, content) {
            return Some(errors);
        }

        if let Some(query) = language.validation_query() {
            let validation_result = ContextValidator::validate_tree(tree, query, content);
            if !validation_result.is_valid {
                return Some(validation_result.format_errors());
            }
        }

        None
    }

    /// Render all tree-sitter ERROR/MISSING regions with context, or `None`
    /// if the tree parses cleanly. Used both for refusals from valid sources
    /// and advisory diagnostics while repairing broken sources.
    fn syntax_errors(language: &LanguageCommon, tree: &Tree, content: &str) -> Option<String> {
        let errors = language.editor().collect_errors(tree, content);
        if errors.is_empty() {
            return None;
        }

        let lines_with_errors = errors.into_iter().collect::<BTreeSet<_>>();
        Some(
            iter::once(String::from("===SYNTAX ERRORS===\n"))
                .chain(excerpt(content, &lines_with_errors))
                .collect(),
        )
    }

    /// The candidates to try, in order, each laid out at its target: targeting,
    /// then [layout](crate::layout), which runs whether or not a formatter will.
    fn build_edits<'editor>(&'editor self) -> Result<Vec<Edit<'editor, 'language>>, String> {
        let edits = if self.starts_with_syntax_errors() {
            EditIterator::new(self).find_text_edits()?
        } else {
            self.language.editor().build_edits(self)?
        };
        Ok(crate::layout::lay_out(self, edits))
    }

    /// Try the candidates in order. The first that validates wins, and the
    /// file's formatter, if it has one, then runs on it alone: it may veto the
    /// edit, but never moves it to another candidate.
    fn edit(&mut self) -> Result<Outcome> {
        let mut edits = match self.build_edits() {
            Ok(all_edits) => all_edits,
            Err(message) => return Ok(Outcome::Rejected(message)),
        };

        // let count = edits.len();
        // edits.dedup();

        // let count_after = edits.len();
        // if count != count_after {
        //     log::trace!("deduped from {count} to {count_after}");
        // }

        for edit in &mut edits {
            if edit.apply() {
                log::trace!("using {edit:#?}");
                if let Some(annotation) = edit.annotation() {
                    log::info!("used {annotation}");
                }
                let ambiguity_note = self.ambiguity_note(edit);
                // The result is the source with the candidate's text in place
                // of what it replaces, starting where the candidate does.
                let span = edit.start_byte()..edit.start_byte() + edit.content().len();
                let output = edit.take_output().unwrap_or_default();
                return Ok(match self.format(output, span) {
                    Formatted::Written { output, whitespace } => Outcome::Applied {
                        output,
                        whitespace,
                        ambiguity_note,
                    },
                    Formatted::Vetoed(message) => Outcome::Rejected(message),
                });
            }
        }

        log::trace!("{edits:#?}");

        // No candidate applied cleanly. Prefer reporting a candidate whose result
        // at least *parsed* (its placement was plausible, so its failure explains
        // what actually blocked the edit) over one that produced structural
        // nonsense — the first candidate is often an inner-node splice whose
        // syntax dump misleads more than it informs. If there were no candidates
        // at all, the anchor matched no editable location — surface that rather
        // than panicking (this runs in-process in the host harness).
        let index = edits.iter().position(Edit::structurally_valid).unwrap_or(0);
        let message = match edits.get_mut(index) {
            Some(edit) => {
                let mut message = edit.take_message().unwrap_or_default();
                if index == 0 && !edit.structurally_valid() {
                    message.push_str(
                        "\n\nNone of the candidate placements for this anchor produced a \
valid file. The diff above shows one failed attempt — if the placement looks wrong, the anchor \
is likely resolving to a different node than intended; try anchoring on the first line of the \
item you mean to target.",
                    );
                }
                message
            }
            None => match self.selector.anchor() {
                Some(anchor) => format!(
                    "No editable location found for anchor {anchor:?}. The file was not modified."
                ),
                None => "No editable location found for that target. The file was not modified."
                    .to_string(),
            },
        };
        Ok(Outcome::Rejected(message))
    }

    /// When the winning edit's anchor matched at more than one location, describe
    /// the alternatives so a first-match resolution never happens silently: the
    /// caller can see at a glance whether the edit landed where they meant it to,
    /// and knows to extend the anchor if it didn't.
    fn ambiguity_note(&self, edit: &Edit) -> Option<String> {
        let (hit_start, _) = *edit.anchor_hit()?;
        let hits = find_positions(&self.source_code, self.selector.anchor()?.trim()).ok()?;
        if hits.len() < 2 {
            return None;
        }
        let line_of = |byte: usize| {
            self.source_code[..byte]
                .bytes()
                .filter(|b| *b == b'\n')
                .count()
                + 1
        };
        let lines = hits
            .iter()
            .map(|(start, _)| line_of(*start).to_string())
            .collect::<Vec<_>>()
            .join(", ");
        Some(format!(
            "Note: the anchor matched at {} locations (lines {lines}); this edit targeted the \
match at line {}. If a different location was intended, extend the anchor with more of the \
target's surrounding text to disambiguate.",
            hits.len(),
            line_of(hit_start),
        ))
    }

    /// For an item-targeted edit, what the item reference resolved to and what the
    /// edit now sits between. An item reference is checked by the caller against a
    /// file it is not looking at, so the placement has to be stated: which item,
    /// which lines, how many of its decorations were taken in, and the neighbor on
    /// the side the content went. `None` for a text anchor, and for a reference
    /// that no longer resolves (which cannot happen on the success path, since the
    /// edit resolved it to run at all).
    fn item_placement_note(&self) -> Option<String> {
        let item = self.selector.item.as_ref()?;
        let resolved = crate::item::resolve(&self.tree, &self.source_code, item).ok()?;
        let (first, last) = resolved.lines;
        let decorations = match resolved.decorations {
            0 => String::from("no leading attributes or doc comments"),
            1 => String::from("with its 1 leading attribute or doc comment"),
            n => format!("with its {n} leading attributes and doc comments"),
        };
        let placement = match self.selector.operation {
            Operation::Replace => format!("Replaced {}.", resolved.description),
            Operation::InsertBefore => match &resolved.previous {
                Some(previous) => format!(
                    "Inserted above it — between {previous} and {}, outside that item's \
decorations, which stay attached to it.",
                    resolved.description
                ),
                None => format!(
                    "Inserted above it, outside its decorations, which stay attached to it; \
{} was the first item in its scope.",
                    resolved.description
                ),
            },
            Operation::InsertAfter => match &resolved.next {
                Some(next) => format!(
                    "Inserted below it — between {} and {next}.",
                    resolved.description
                ),
                None => format!(
                    "Inserted below it; {} was the last item in its scope.",
                    resolved.description
                ),
            },
        };
        Some(format!(
            "Item target: {} at lines {first}-{last}, {decorations}. {placement}",
            resolved.description
        ))
    }

    /// A candidate shorter anchor for the same target: the first line of the
    /// current anchor, offered only when the anchor is multi-line and the
    /// first line alone matches exactly once in the file. This is a
    /// *candidate* — callers teaching it as equivalent must verify it by
    /// re-running the edit and comparing diffs (see
    /// [`equivalent_diffs`](Self::equivalent_diffs)).
    pub fn shorthand_suggestion(&self) -> Option<String> {
        let anchor = self.selector.anchor()?.trim();
        let first_line = anchor.lines().next()?.trim();
        if first_line == anchor || first_line.is_empty() {
            return None;
        }
        let hits = find_positions(&self.source_code, first_line).ok()?;
        (hits.len() == 1).then(|| first_line.to_string())
    }

    /// Whether two preview messages describe the identical change: their
    /// `===DIFF===` sections match, ignoring any trailing ambiguity note.
    pub fn equivalent_diffs(a: &str, b: &str) -> bool {
        fn diff_section(message: &str) -> Option<&str> {
            let section = &message[message.find("===DIFF===")?..];
            let end = section
                .find("\n\nNote: the anchor matched")
                .unwrap_or(section.len());
            Some(&section[..end])
        }
        matches!((diff_section(a), diff_section(b)), (Some(a), Some(b)) if a == b)
    }

    /// Run the edit against the in-memory source. Nothing is written here: on
    /// success the report (what happened to the edit's whitespace, the diff,
    /// the ambiguity note) comes back with the [`AppliedEdit`] record, and the
    /// *caller* persists the record's `post_edit_source` and prepends its own
    /// headline. On failure, including the formatter's veto, the failure report
    /// comes back alone and there is nothing to persist.
    pub fn apply(mut self) -> Result<(String, Option<AppliedEdit>)> {
        match self.edit()? {
            Outcome::Applied {
                output,
                whitespace,
                ambiguity_note,
            } => Ok(self.report(output, whitespace, ambiguity_note)),
            Outcome::Rejected(message) => Ok((message, None)),
        }
    }

    /// The success report for `output`, the winning candidate's result as it is
    /// written, and the record of the edit.
    fn report(
        self,
        output: String,
        whitespace: Whitespace,
        note: Option<String>,
    ) -> (String, Option<AppliedEdit>) {
        // Only a formatter that may change the whole file can have changed
        // lines far from the edit. Otherwise every hunk is the edit's own.
        let whole_file = matches!(whitespace, Whitespace::Formatted(_, Scope::File));
        let plain_text = self.language.name() == LanguageName::Other;
        let (diff, elided) = if plain_text {
            (clean_diff(&self.source_code, &output), 0)
        } else {
            focused_diff(&self.source_code, &output, whole_file)
        };
        let tree = self.parse(&output, None);
        let remaining_errors = tree
            .as_ref()
            .and_then(|tree| Self::syntax_errors(self.language, tree, &output));
        // A repair is accepted unchecked. Once it parses again, the language's
        // check still says whether it's valid, as a formatter's refusal did.
        let failed_check = match &tree {
            Some(tree) if remaining_errors.is_none() && self.starts_with_syntax_errors() => {
                self.language.editor().check(tree, &output).err()
            }
            _ => None,
        };
        // A source that already failed the check had it off for this edit
        // (see `check_failure`). That is said whenever the result still fails
        // it: then the check isn't protecting the edit. A result that passes
        // is what the check would have let through anyway.
        let unguarded = match &tree {
            Some(tree) if !self.starts_with_syntax_errors() && !self.passes_check() => {
                self.language.editor().check(tree, &output).err()
            }
            _ => None,
        };
        // What happened to the edit's whitespace, said first unless a
        // warning about the repair it belongs to has to come before it.
        let whitespace_note = match &whitespace {
            Whitespace::Formatted(formatter, Scope::Edit) => {
                format!("Note: {formatter} formatted the edit.\n")
            }
            Whitespace::Formatted(formatter, Scope::File) => {
                format!("Note: {formatter} formatted the whole file, including the edit.\n")
            }
            Whitespace::Unformatted(note) | Whitespace::RepairUnformatted(note) => {
                format!("{note}\n")
            }
            Whitespace::AsLaidOut if plain_text => String::from(
                "Note: plain-text edits preserve supplied whitespace; no formatter is run.\n",
            ),
            Whitespace::AsLaidOut => String::from(
                "Note: no formatter is run on this file; the diff shows exactly what was written.\n",
            ),
        };
        let mut report = if let Some(errors) = remaining_errors {
            format!(
                "Warning: the source already had syntax errors. This edit was accepted without formatting; the resulting file still has syntax errors in the regions below, including any outside the diff.\n\n{errors}"
            )
        } else if matches!(whitespace, Whitespace::RepairUnformatted(_)) {
            whitespace_note
        } else if let Some(diagnostic) = failed_check {
            let mut warning = format!(
                "Warning: the source already had syntax errors, so this edit was accepted without validation. The result parses again but has the error below, and until it is fixed, edits to this file aren't checked for errors like it.\n\n{}",
                check_errors(&diagnostic, &output)
            );
            // Why the formatter wasn't used is said whatever else is.
            if matches!(whitespace, Whitespace::Unformatted(_)) {
                warning.push_str(&whitespace_note);
            }
            warning
        } else {
            whitespace_note
        };
        if elided > 0 {
            report.push_str(&format!(
                "{elided} formatting-only hunk{} elsewhere in the file {} not shown \
(whitespace only — no token changed; anchors are whitespace-insensitive, so later edits are \
unaffected).\n",
                if elided == 1 { "" } else { "s" },
                if elided == 1 { "is" } else { "are" },
            ));
        }
        if let Some(diagnostic) = unguarded {
            report.push_str(&format!(
                "Note: the syntax check was off for this edit, because the file already \
failed it; the result fails it too: {diagnostic}\n"
            ));
        }
        if let Some(placement) = self.item_placement_note() {
            report.push_str(&placement);
            report.push('\n');
        }
        report.push('\n');
        report.push_str(&diff);
        if let Some(note) = note {
            report.push_str("\n\n");
            report.push_str(&note);
        }

        let Self {
            content,
            selector,
            file_path,
            language,
            source_code,
            ..
        } = self;
        let record = AppliedEdit {
            operation: EditOperation {
                selector,
                content,
                file_path,
                language_name: language.name(),
            },
            pre_edit_source: source_code,
            post_edit_source: output,
        };
        (report, Some(record))
    }

    /// Show every location the anchor matches, with surrounding context.
    ///
    /// This deliberately reports *text matches*, not nodes: the node an edit
    /// ultimately operates on depends on the operation and content of that
    /// edit, which this tool doesn't have — so any node it displayed could
    /// disagree with what a later edit picks. Match locations, by contrast,
    /// use the same [`find_positions`] search the editor uses, so what this
    /// shows is exactly what an edit will find.
    pub fn find_anchor(self) -> Result<String> {
        let anchor = self.selector.anchor().unwrap_or_default().trim();
        let positions = match find_positions(&self.source_code, anchor) {
            Ok(positions) => positions,
            Err(message) => return Ok(message),
        };

        // 0-indexed line containing `byte`
        let line_of = |byte: usize| {
            self.source_code[..byte]
                .bytes()
                .filter(|b| *b == b'\n')
                .count()
        };
        let all_lines: Vec<&str> = self.source_code.lines().collect();
        let context = 3;

        let mut output = format!(
            "Anchor matched at {} location{}\n",
            positions.len(),
            if positions.len() == 1 { "" } else { "s" }
        );

        for (index, (start, end)) in positions.iter().enumerate() {
            let start_line = line_of(*start);
            let end_line = line_of(end.saturating_sub(1).max(*start));
            output.push_str(&format!(
                "\n=== match {}: lines {}-{} ===\n",
                index + 1,
                start_line + 1,
                end_line + 1
            ));

            let context_start = start_line.saturating_sub(context);
            let context_end = (end_line + context + 1).min(all_lines.len());
            for (offset, line) in all_lines[context_start..context_end].iter().enumerate() {
                let line_index = context_start + offset;
                let marker = if (start_line..=end_line).contains(&line_index) {
                    ">>>"
                } else {
                    "   "
                };
                output.push_str(&format!("{} {:4} | {}\n", marker, line_index + 1, line));
            }
        }

        if positions.len() > 1 {
            output.push_str(
                "\nEdits use the first match. To target a different one, extend the anchor \
with more of the target's own text until it is unique.",
            );
        }

        Ok(output)
    }

    fn diff(&self, output: &str) -> String {
        let source_code: &str = &self.source_code;
        let mut cleaned_diff = String::new();
        cleaned_diff.push_str(&clean_diff(source_code, output));
        cleaned_diff
    }

    fn parse(&self, output: &str, old_tree: Option<&Tree>) -> Option<Tree> {
        // A parser-construction failure (e.g. a tree-sitter ABI mismatch) folds into
        // the existing "couldn't parse" path rather than panicking the host process.
        let mut parser = self.language.tree_sitter_parser().ok()?;
        parser.parse(output, old_tree)
    }
}

/// The lines of `content` around each of `marked` (indexes from 0), numbered,
/// with the marked ones pointed out.
fn excerpt<'a>(content: &'a str, marked: &'a BTreeSet<usize>) -> impl Iterator<Item = String> + 'a {
    let context_lines = 3;
    let shown = marked
        .iter()
        .copied()
        .flat_map(|line| line.saturating_sub(context_lines)..line + context_lines)
        .collect::<BTreeSet<_>>();
    content
        .lines()
        .enumerate()
        .filter(move |(index, _)| shown.contains(index))
        .map(|(index, line)| {
            let display_index = index + 1;
            if marked.contains(&index) {
                format!("{display_index:>4} ->⎸{line}\n")
            } else {
                format!("{display_index:>4}   ⎸{line}\n")
            }
        })
}

/// A language check's rejection of `content`: the checker's diagnostic, then
/// the lines around the one it points at.
fn check_errors(diagnostic: &Diagnostic, content: &str) -> String {
    let mut errors = format!("===SYNTAX ERROR===\n{diagnostic}\n");
    if let Some(line) = diagnostic.line {
        let last = content.lines().count().max(1);
        let marked = BTreeSet::from([line.clamp(1, last) - 1]);
        errors.extend(excerpt(content, &marked));
    }
    errors
}

/// A `===DIFF===` section between two versions of a file, cleaned for AI
/// consumption: the `---`/`+++` file headers and newline metadata are stripped.
///
/// Hunk headers are kept. They are the only line numbers in the result, and an
/// edit result is exactly where a caller re-targets from — without them a
/// multi-hunk diff reads as one contiguous block of changes that are not
/// adjacent in the file.
pub(crate) fn clean_diff(original: &str, modified: &str) -> String {
    let diff_patch = DiffOptions::new().create_patch(original, modified);
    let formatter = PatchFormatter::new().missing_newline_message(false);
    let diff_output = formatter.fmt_patch(&diff_patch).to_string();

    let mut cleaned_diff = String::from("===DIFF===\n");
    for line in diff_output.lines() {
        if line.starts_with("---") || line.starts_with("+++") {
            continue;
        }
        // A blank context line has no prefix (`PatchFormatter`'s
        // `suppress_blank_empty`), but diffy sees a CRLF file's blank line as
        // `\r`, prefixes it, and `lines` leaves just the prefix.
        if line != " " {
            cleaned_diff.push_str(line);
        }
        cleaned_diff.push('\n');
    }

    // Remove trailing newline to avoid extra spacing
    if cleaned_diff.ends_with('\n') {
        cleaned_diff.pop();
    }
    cleaned_diff
}

/// The same `===DIFF===` section with the **whitespace-only hunks dropped**, plus
/// the number dropped, when `whole_file` says a formatter that may change the
/// whole file ([`Scope::File`]) produced `modified`.
///
/// Such a formatter's changes can reach arbitrarily far from the edit, as churn in
/// a file that wasn't already formatted — on a large file that buries the semantic
/// change and costs a round of re-inspecting code the caller never touched.
/// Without one, nothing outside the edit and the lines beside it changed, so every
/// hunk is the edit's own, whitespace included, and none is dropped.
///
/// The classification is against the formatter's own contribution, hunk by hunk: a
/// hunk whose deleted and inserted text are identical once all whitespace is
/// removed changed no token, and is therefore formatting. Anything else stays,
/// wherever it is in the file — a formatter that rewrites a token (a quote style,
/// a dropped import, an added semicolon) is a semantic difference between the
/// caller's model and the bytes on disk, and hiding it would let the caller
/// continue from a file it no longer describes. If nothing survives the filter
/// (an edit that was itself whitespace-only), the full diff is returned instead,
/// because an empty diff answers nothing.
pub(crate) fn focused_diff(original: &str, modified: &str, whole_file: bool) -> (String, usize) {
    let patch = DiffOptions::new().create_patch(original, modified);
    let mut kept = String::from("===DIFF===\n");
    let mut elided = 0;
    let mut shown = 0;

    for hunk in patch.hunks() {
        if whole_file && hunk_is_whitespace_only(hunk) {
            elided += 1;
            continue;
        }
        shown += 1;
        kept.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            hunk.old_range().start(),
            hunk.old_range().len(),
            hunk.new_range().start(),
            hunk.new_range().len()
        ));
        for line in hunk.lines() {
            let (prefix, text) = match line {
                DiffyLine::Context(text) => (' ', *text),
                DiffyLine::Delete(text) => ('-', *text),
                DiffyLine::Insert(text) => ('+', *text),
            };
            // Each line without its line ending, as `clean_diff`'s are: a CRLF
            // file's diff reads like any other.
            let text = text.strip_suffix('\n').unwrap_or(text);
            let text = text.strip_suffix('\r').unwrap_or(text);
            // Mirrors `PatchFormatter`'s `suppress_blank_empty` (its default, and
            // what every existing snapshot was written against): a blank context
            // line carries no ` ` prefix, so the diff has no trailing whitespace.
            if !(prefix == ' ' && text.is_empty()) {
                kept.push(prefix);
            }
            kept.push_str(text);
            kept.push('\n');
        }
    }

    if shown == 0 {
        return (clean_diff(original, modified), 0);
    }

    if kept.ends_with('\n') {
        kept.pop();
    }
    (kept, elided)
}

/// Whether a hunk's deletions and insertions are the same text once every
/// whitespace character is removed — i.e. whether the formatter, and not the edit,
/// accounts for it. See [`focused_diff`].
fn hunk_is_whitespace_only(hunk: &diffy::Hunk<'_, str>) -> bool {
    fn significant(hunk: &diffy::Hunk<'_, str>, inserted: bool) -> String {
        hunk.lines()
            .iter()
            .filter_map(|line| match (line, inserted) {
                (DiffyLine::Delete(text), false) | (DiffyLine::Insert(text), true) => Some(*text),
                _ => None,
            })
            .flat_map(str::chars)
            .filter(|c| !c.is_whitespace())
            .collect()
    }
    significant(hunk, false) == significant(hunk, true)
}

#[cfg(test)]
mod diff_tests {
    use super::*;

    /// Formatter churn far from the edit is whitespace-only, so it is dropped and
    /// counted — the semantic change is what the caller reads.
    #[test]
    fn a_distant_whitespace_only_hunk_is_elided_and_counted() {
        let original = "fn a() {\n        let x = 1;\n}\n".to_string()
            + &"// filler\n".repeat(20)
            + "struct S {\n    a: u8,\n}\n";
        let modified = "fn a() {\n    let x = 1;\n}\n".to_string()
            + &"// filler\n".repeat(20)
            + "struct S {\n    a: u8,\n    b: u8,\n}\n";

        let (diff, elided) = focused_diff(&original, &modified, true);
        assert_eq!(elided, 1, "{diff}");
        assert!(diff.contains("+    b: u8,"), "the edit is shown: {diff}");
        assert!(!diff.contains("let x = 1"), "reflow is not: {diff}");
    }

    /// A change outside the edit that touches a *token* is not formatting, whatever
    /// its distance: hiding it would let the caller continue from a file it no
    /// longer describes.
    #[test]
    fn a_distant_non_whitespace_change_always_stays() {
        let original = "use a::b;\n".to_string() + &"// filler\n".repeat(20) + "struct S {\n}\n";
        let modified =
            "use a::c;\n".to_string() + &"// filler\n".repeat(20) + "struct S {\n    a: u8,\n}\n";

        let (diff, elided) = focused_diff(&original, &modified, true);
        assert_eq!(elided, 0, "{diff}");
        assert!(diff.contains("+use a::c;"), "{diff}");
        assert!(diff.contains("+    a: u8,"), "{diff}");
    }

    /// If the filter would leave nothing — an edit that was itself whitespace-only
    /// — the full diff is returned: an empty diff answers nothing.
    #[test]
    fn an_all_whitespace_diff_falls_back_to_the_full_diff() {
        let (diff, elided) = focused_diff("fn a() {\n  x\n}\n", "fn a() {\n    x\n}\n", true);
        assert_eq!(elided, 0);
        assert!(diff.contains("+    x"), "{diff}");
    }

    /// Without a formatter that may change the whole file, a whitespace-only hunk
    /// is the edit's own: dropping it would hide what the edit did to the file and
    /// claim a formatter did it.
    #[test]
    fn an_unformatted_whitespace_only_hunk_is_shown() {
        let original = "fn a() {\n        let x = 1;\n}\n".to_string()
            + &"// filler\n".repeat(20)
            + "struct S {\n    a: u8,\n}\n";
        let modified = "fn a() {\n    let x = 1;\n}\n".to_string()
            + &"// filler\n".repeat(20)
            + "struct S {\n    a: u8,\n    b: u8,\n}\n";

        let (diff, elided) = focused_diff(&original, &modified, false);
        assert_eq!(elided, 0, "{diff}");
        assert!(diff.contains("+    let x = 1;"), "{diff}");
        assert!(diff.contains("+    b: u8,"), "{diff}");
    }

    /// Hunk headers are the result's only line numbers, and an edit result is where
    /// a caller re-targets from.
    #[test]
    fn hunks_carry_their_line_numbers() {
        let (diff, _) = focused_diff("a\nb\nc\n", "a\nB\nc\n", true);
        assert!(diff.contains("@@ -1,3 +1,3 @@"), "{diff}");
    }

    /// A blank context line carries no prefix, matching `PatchFormatter`'s default —
    /// the diff has no trailing whitespace.
    #[test]
    fn a_blank_context_line_has_no_prefix() {
        let (diff, _) = focused_diff("a\n\nb\n", "a\n\nB\n", true);
        assert!(diff.contains("\n\n"), "blank line stays bare: {diff:?}");
        assert!(!diff.contains(" \n"), "no trailing space: {diff:?}");
    }

    /// A CRLF file's diff reads like any other, as the full diff's does.
    #[test]
    fn a_crlf_diff_has_no_carriage_returns() {
        let (original, modified) = ("a\r\n\r\nb\r\n", "a\r\n\r\nB\r\n");
        let (diff, _) = focused_diff(original, modified, false);
        assert_eq!(diff, "===DIFF===\n@@ -1,3 +1,3 @@\n a\n\n-b\n+B");
        assert_eq!(diff, clean_diff(original, modified));
    }
}
