mod edit;
mod edit_iterator;
mod edit_position;

use crate::{
    languages::{LanguageCommon, LanguageRegistry},
    selector::Selector,
    state::StagedOperation,
    validation::{ContextValidator, format_violations},
};
use anyhow::{Result, anyhow};
use diffy::{DiffOptions, Patch, PatchFormatter};
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
    staged_edit: Option<EditPosition>,
}

impl<'language> Editor<'language> {
    pub fn new(
        content: String,
        selector: Selector,
        language: &'language LanguageCommon,
        file_path: PathBuf,
        staged_edit: Option<EditPosition>,
    ) -> Result<Self> {
        let source_code = std::fs::read_to_string(&file_path)?;
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
            staged_edit,
        })
    }

    pub fn from_staged_operation(
        staged_operation: StagedOperation,
        language_registry: &'language LanguageRegistry,
    ) -> Result<Self> {
        let StagedOperation {
            selector,
            content,
            file_path,
            language_name,
            edit_position,
        } = staged_operation;
        let language = language_registry.get_language(language_name);
        Self::new(content, selector, language, file_path, edit_position)
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

    fn edit(&mut self) -> Result<(String, Option<String>)> {
        if let Some(prevalidation_failure) = self.prevalidate() {
            return Ok((prevalidation_failure, None));
        };

        let mut edits = match self.build_edits() {
            Ok(all_edits) => all_edits,
            Err(message) => return Ok((message, None)),
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
                return Ok((edit.take_message().unwrap_or_default(), edit.take_output()));
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
            None => format!(
                "No editable location found for anchor {:?}. The file was not modified.",
                self.selector.anchor
            ),
        };
        Ok((message, None))
    }

    pub fn preview(mut self) -> Result<(String, Option<StagedOperation>)> {
        let (message, output) = self.edit()?;
        if let Some(output) = &output {
            let mut preview = String::new();

            preview.push_str(&format!(
                "Previewing: {}\nNote: the editor applies a consistent formatting style to the entire file, including your edit\n\n",
                self.selector.operation_name()
            ));
            preview.push_str(&self.diff(output));

            Ok((preview, Some(self.into())))
        } else {
            Ok((message, None))
        }
    }
    pub fn read_node(self) -> Result<String> {
        // Build edits to find nodes matching the anchor
        let mut edits = match self.build_edits() {
            Ok(all_edits) => all_edits,
            Err(message) => return Ok(message),
        };

        if edits.is_empty() {
            return Ok(format!(
                "No node found for anchor {:?}",
                self.selector.anchor
            ));
        }

        // Use the first edit to get node information
        let edit = edits.remove(0);
        let position = edit.position();

        // Get the text of the node
        let start_byte = position.start_byte();
        let end_byte = position.end_byte().unwrap_or(start_byte);
        let node_text = self.source_code.get(start_byte..end_byte).unwrap_or("");

        // Calculate line numbers for context
        let line_count = self.source_code[..start_byte].lines().count();
        let start_line = line_count;
        let end_line = start_line + node_text.lines().count() - 1;

        // Get surrounding context (5 lines before and after)
        let context_lines = 5;
        let all_lines: Vec<&str> = self.source_code.lines().collect();

        let context_start = start_line.saturating_sub(context_lines);
        let context_end = (end_line + context_lines + 1).min(all_lines.len());

        let mut output = String::new();
        output.push_str(&format!(
            "Found node at lines {}-{}\n\n",
            start_line + 1,
            end_line + 1
        ));
        output.push_str("===CONTEXT===\n");

        for (idx, line) in all_lines[context_start..context_end].iter().enumerate() {
            let line_num = context_start + idx + 1;
            let marker = if line_num > start_line && line_num <= end_line + 1 {
                ">>>"
            } else {
                "   "
            };
            output.push_str(&format!("{} {:4} | {}\n", marker, line_num, line));
        }

        Ok(output)
    }

    fn diff(&self, output: &str) -> String {
        let source_code: &str = &self.source_code;
        let content_patch = &self.content;
        let diff_patch = DiffOptions::new().create_patch(source_code, output);
        let formatter = PatchFormatter::new().missing_newline_message(false);

        // Get the diff string and clean it up for AI consumption
        let diff_output = formatter.fmt_patch(&diff_patch).to_string();
        let lines: Vec<&str> = diff_output.lines().collect();
        let mut cleaned_diff = String::new();

        let content_line_count = content_patch.lines().count();
        if content_line_count > 10 {
            let changed_lines = changed_lines(&diff_patch, content_line_count);

            let changed_fraction = (changed_lines * 100) / content_line_count;

            if changed_fraction < 30 {
                cleaned_diff.push_str("💡 TIP: For focused changes like this, you might try targeted insert/replace operations for easier review and iteration\n");
            };
            cleaned_diff.push('\n');
        }

        cleaned_diff.push_str("===DIFF===\n");
        for line in lines {
            // Skip ALL diff headers: file headers, hunk headers (line numbers), and any metadata
            if line.starts_with("---") || line.starts_with("+++") || line.starts_with("@@") {
                // Skip "\ No newline at end of file" messages
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

    pub fn commit(mut self) -> Result<(String, Option<String>, PathBuf)> {
        let (mut message, output) = self.edit()?;
        if let Some(output) = &output {
            let diff = self.diff(output);

            message = format!(
                "{} operation result:\n{}\n\n{diff}",
                self.selector.operation_name(),
                message,
            );
        }
        Ok((message, output, self.file_path))
    }

    fn parse(&self, output: &str, old_tree: Option<&Tree>) -> Option<Tree> {
        // A parser-construction failure (e.g. a tree-sitter ABI mismatch) folds into
        // the existing "couldn't parse" path rather than panicking the host process.
        let mut parser = self.language.tree_sitter_parser().ok()?;
        parser.parse(output, old_tree)
    }
}

impl From<Editor<'_>> for StagedOperation {
    fn from(value: Editor) -> Self {
        let Editor {
            content,
            selector,
            file_path,
            language,
            staged_edit,
            ..
        } = value;
        Self {
            selector,
            content,
            file_path,
            language_name: language.name(),
            edit_position: staged_edit,
        }
    }
}

pub fn changed_lines(patch: &Patch<'_, str>, content_line_count: usize) -> usize {
    let mut changed_line_numbers = BTreeSet::new();

    for hunk in patch.hunks() {
        // old_range().range() returns a std::ops::Range<usize> that's properly 0-indexed
        for line_num in hunk.old_range().range() {
            if line_num < content_line_count {
                changed_line_numbers.insert(line_num);
            }
        }
    }
    changed_line_numbers.len()
}
