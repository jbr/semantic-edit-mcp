mod edit;
mod edit_iterator;
mod edit_position;

use crate::{
    languages::LanguageCommon,
    searcher::find_positions,
    selector::{Operation, Selector},
    state::{AppliedEdit, EditOperation},
    validation::{ContextValidator, format_violations},
};
use anyhow::{Result, anyhow};
use diffy::{DiffOptions, Line as DiffyLine, PatchFormatter};
use ropey::Rope;
use std::{
    collections::{BTreeMap, BTreeSet},
    iter,
    path::PathBuf,
};
use tree_sitter::Tree;

pub(crate) use edit::Edit;
pub(crate) use edit_iterator::EditIterator;
pub(crate) use edit_position::EditPosition;

/// Why a candidate's result tree was rejected. `syntax: true` means the result
/// didn't even parse cleanly (the placement itself is structural nonsense);
/// `syntax: false` means it parsed but introduced a context-query violation.
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
    source_code: String,
    tree: Tree,
    rope: Rope,
}

impl<'language> Editor<'language> {
    pub fn new(
        content: String,
        selector: Selector,
        language: &'language LanguageCommon,
        file_path: PathBuf,
    ) -> Result<Self> {
        let source_code = std::fs::read_to_string(&file_path)?;
        Self::from_source(content, selector, language, file_path, source_code)
    }

    /// Construct against an explicit source text instead of reading
    /// `file_path` from disk. Retargeting uses this to re-run an edit against
    /// the recorded *pre-edit* source while the file on disk already contains
    /// the edit being moved.
    pub fn from_source(
        content: String,
        selector: Selector,
        language: &'language LanguageCommon,
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
            tree,
            file_path,
            source_code,
            rope,
        })
    }

    fn prevalidate(&self) -> Option<String> {
        // Only gate *entry* on genuine syntax errors (ERROR/MISSING nodes). The
        // heuristic context-validation queries describe whether the *result* of an
        // edit is well-formed, not the pre-existing file — running them here would
        // let any pre-existing flagged-but-valid construct (e.g. a module-level
        // `static mut`) block every edit to the file.
        Self::syntax_errors(self.language, &self.tree, &self.source_code).map(|errors| {
            format!(
                "Syntax error found prior to edit, not attempting.
Suggestion: Pause and show your human collaborator this context:\n\n{errors}"
            )
        })
    }

    /// Validate a candidate result tree, relative to the file being edited.
    ///
    /// Unlike the absolute [`validate`](Self::validate), context-query violations
    /// are compared against the *original* source: a violation that already
    /// exists in the unedited file is pre-existing code, not something this edit
    /// introduced, so it never fails the candidate. Without this, one imperfect
    /// heuristic match anywhere in a file makes the whole file uneditable — and
    /// the resulting error points at code the caller never touched.
    fn validate_tree(&self, tree: &Tree, content: &str) -> Option<ValidationFailure> {
        if let Some(errors) = Self::syntax_errors(self.language, tree, content) {
            return Some(ValidationFailure {
                message: errors,
                syntax: true,
            });
        }

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

    /// Render a context-annotated report of tree-sitter ERROR/MISSING nodes, or
    /// `None` if the tree parses cleanly. This is the pure-syntax half of
    /// [`validate`](Self::validate) — used by [`prevalidate`](Self::prevalidate) to
    /// gate edits without invoking the heuristic context queries.
    fn syntax_errors(language: &LanguageCommon, tree: &Tree, content: &str) -> Option<String> {
        let errors = language.editor().collect_errors(tree, content);
        if errors.is_empty() {
            return None;
        }

        let context_lines = 3;
        let lines_with_errors = errors.into_iter().collect::<BTreeSet<_>>();
        let context_lines = lines_with_errors
            .iter()
            .copied()
            .flat_map(|line| line.saturating_sub(context_lines)..line + context_lines)
            .collect::<BTreeSet<_>>();
        Some(
            iter::once(String::from("===SYNTAX ERRORS===\n"))
                .chain(
                    content
                        .lines()
                        .enumerate()
                        .filter(|(index, _)| context_lines.contains(index))
                        .map(|(index, line)| {
                            let display_index = index + 1;
                            if lines_with_errors.contains(&index) {
                                format!("{display_index:>4} ->⎸{line}\n")
                            } else {
                                format!("{display_index:>4}   ⎸{line}\n")
                            }
                        }),
                )
                .collect(),
        )
    }

    fn build_edits<'editor>(&'editor self) -> Result<Vec<Edit<'editor, 'language>>, String> {
        self.language.editor().build_edits(self)
    }

    /// Returns `(message, output, ambiguity_note)` — `output` is `Some` only when
    /// a candidate applied cleanly, and `ambiguity_note` is `Some` only when the
    /// winning candidate's anchor also matched elsewhere in the file.
    fn edit(&mut self) -> Result<(String, Option<String>, Option<String>)> {
        if let Some(prevalidation_failure) = self.prevalidate() {
            return Ok((prevalidation_failure, None, None));
        };

        let mut edits = match self.build_edits() {
            Ok(all_edits) => all_edits,
            Err(message) => return Ok((message, None, None)),
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
                let note = self.ambiguity_note(edit);
                return Ok((
                    edit.take_message().unwrap_or_default(),
                    edit.take_output(),
                    note,
                ));
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
        Ok((message, None, None))
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
    /// success the report (formatting note, diff, ambiguity note) comes back
    /// with the [`AppliedEdit`] record, and the *caller* persists the record's
    /// `post_edit_source` and prepends its own headline. On failure the
    /// failure report comes back alone and there is nothing to persist.
    pub fn apply(mut self) -> Result<(String, Option<AppliedEdit>)> {
        let (message, output, note) = self.edit()?;
        if let Some(output) = output {
            let (diff, elided) = focused_diff(&self.source_code, &output);
            let mut report = String::from(
                "Note: the editor applies a consistent formatting style to the entire file, including your edit\n",
            );
            if elided > 0 {
                report.push_str(&format!(
                    "{elided} formatting-only hunk{} elsewhere in the file {} not shown \
(whitespace only — no token changed; anchors are whitespace-insensitive, so later edits are \
unaffected).\n",
                    if elided == 1 { "" } else { "s" },
                    if elided == 1 { "is" } else { "are" },
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
            Ok((report, Some(record)))
        } else {
            Ok((message, None))
        }
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

    pub fn format_code(&self, source: &str) -> Result<String, String> {
        self.language
            .editor()
            .format_code(source, &self.file_path)
            .map_err(|e| {
                let diff = self.diff(source);
                format!(
                    "The formatter has encountered the following error making \
                 that change, so the file has not been modified. The tool has \
                 prevented what it believes to be an unsafe edit. Please try a \
                 different edit.\n\n\
                 {e}\n\n{diff}"
                )
            })
    }

    fn parse(&self, output: &str, old_tree: Option<&Tree>) -> Option<Tree> {
        // A parser-construction failure (e.g. a tree-sitter ABI mismatch) folds into
        // the existing "couldn't parse" path rather than panicking the host process.
        let mut parser = self.language.tree_sitter_parser().ok()?;
        parser.parse(output, old_tree)
    }
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
        cleaned_diff.push_str(line);
        cleaned_diff.push('\n');
    }

    // Remove trailing newline to avoid extra spacing
    if cleaned_diff.ends_with('\n') {
        cleaned_diff.pop();
    }
    cleaned_diff
}

/// The same `===DIFF===` section with the **whitespace-only hunks dropped**, plus
/// the number dropped.
///
/// An edit is validated by formatting the whole file, so the file-level diff can
/// carry formatter churn arbitrarily far from the edit — on a large file that
/// buries the semantic change and costs a round of re-inspecting code the caller
/// never touched.
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
pub(crate) fn focused_diff(original: &str, modified: &str) -> (String, usize) {
    let patch = DiffOptions::new().create_patch(original, modified);
    let mut kept = String::from("===DIFF===\n");
    let mut elided = 0;
    let mut shown = 0;

    for hunk in patch.hunks() {
        if hunk_is_whitespace_only(hunk) {
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
            // Mirrors `PatchFormatter`'s `suppress_blank_empty` (its default, and
            // what every existing snapshot was written against): a blank context
            // line carries no ` ` prefix, so the diff has no trailing whitespace.
            if !(prefix == ' ' && text == "\n") {
                kept.push(prefix);
            }
            kept.push_str(text);
            if !text.ends_with('\n') {
                kept.push('\n');
            }
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

        let (diff, elided) = focused_diff(&original, &modified);
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

        let (diff, elided) = focused_diff(&original, &modified);
        assert_eq!(elided, 0, "{diff}");
        assert!(diff.contains("+use a::c;"), "{diff}");
        assert!(diff.contains("+    a: u8,"), "{diff}");
    }

    /// If the filter would leave nothing — an edit that was itself whitespace-only
    /// — the full diff is returned: an empty diff answers nothing.
    #[test]
    fn an_all_whitespace_diff_falls_back_to_the_full_diff() {
        let (diff, elided) = focused_diff("fn a() {\n  x\n}\n", "fn a() {\n    x\n}\n");
        assert_eq!(elided, 0);
        assert!(diff.contains("+    x"), "{diff}");
    }

    /// Hunk headers are the result's only line numbers, and an edit result is where
    /// a caller re-targets from.
    #[test]
    fn hunks_carry_their_line_numbers() {
        let (diff, _) = focused_diff("a\nb\nc\n", "a\nB\nc\n");
        assert!(diff.contains("@@ -1,3 +1,3 @@"), "{diff}");
    }

    /// A blank context line carries no prefix, matching `PatchFormatter`'s default —
    /// the diff has no trailing whitespace.
    #[test]
    fn a_blank_context_line_has_no_prefix() {
        let (diff, _) = focused_diff("a\n\nb\n", "a\n\nB\n");
        assert!(diff.contains("\n\n"), "blank line stays bare: {diff:?}");
        assert!(!diff.contains(" \n"), "no trailing space: {diff:?}");
    }
}
