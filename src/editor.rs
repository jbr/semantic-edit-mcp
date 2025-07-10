mod edit;
mod edit_iterator;
mod edit_position;

use std::{collections::BTreeSet, path::PathBuf};

use anyhow::{anyhow, Result};
use diffy::{DiffOptions, Patch, PatchFormatter};
pub(crate) use edit::Edit;
pub(crate) use edit_iterator::EditIterator;
use ropey::Rope;
use tree_sitter::Tree;

pub use edit_position::EditPosition;

use crate::{
    languages::{LanguageCommon, LanguageRegistry},
    selector::Selector,
    state::StagedOperation,
    validation::ContextValidator,
};

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
        self.validate_tree(&self.tree, &self.source_code)
            .map(|errors| {
                format!(
                    "Syntax error found prior to edit, not attempting.
Suggestion: Pause and show your human collaborator this context:\n\n{errors}"
                )
            })
    }

    fn validate_tree(&self, tree: &Tree, content: &str) -> Option<String> {
        Self::validate(self.language, tree, content)
    }

    pub fn validate(language: &LanguageCommon, tree: &Tree, content: &str) -> Option<String> {
        let errors = language.editor().collect_errors(tree, content);
        if errors.is_empty() {
            if let Some(query) = language.validation_query() {
                let validation_result = ContextValidator::validate_tree(tree, query, content);

                if !validation_result.is_valid {
                    return Some(validation_result.format_errors());
                }
            }

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
            std::iter::once(String::from("===SYNTAX ERRORS===\n"))
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

        let mut all_edits = match self.build_edits() {
            Ok(all_edits) => all_edits,
            Err(message) => return Ok((message, None)),
        };

        for edit in &mut all_edits {
            if edit.apply()? {
                log::trace!("using {edit:?}");
                if let Some(description) = edit.internal_explanation() {
                    log::info!("used {description}");
                }
                return Ok((edit.take_message().unwrap_or_default(), edit.take_output()));
            }
        }

        log::trace!("{all_edits:#?}");

        Ok((
            all_edits
                .first_mut()
                .unwrap()
                .take_message()
                .unwrap_or_default(),
            None,
        ))
    }

    pub fn preview(mut self) -> Result<(String, Option<StagedOperation>)> {
        let (message, output) = self.edit()?;
        if let Some(output) = &output {
            let mut preview = String::new();

            preview.push_str(&format!("STAGED: {}\n\n", self.selector.operation_name()));
            preview.push_str(&self.diff(output));

            Ok((preview, Some(self.into())))
        } else {
            Ok((message, None))
        }
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

            //            cleaned_diff.push_str(&format!("Edit efficiency: {changed_fraction}%\n",));
            if changed_fraction < 30 {
                cleaned_diff.push_str("💡 TIP: For focused changes like this, you might try targeted insert/replace operations for easier review and iteration\n");
            };
            cleaned_diff.push('\n');
        }

        cleaned_diff.push_str("===DIFF===\nNote: the editor applies a consistent formatting style to the entire file, including your edit");
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

    pub fn format_code(&self, source: &str) -> Result<String> {
        self.language
            .editor()
            .format_code(source, &self.file_path)
            .map_err(|e| {
                anyhow!(
                    "The formatter has encountered the following error making \
                 that change, so the file has not been modified. The tool has \
                 prevented what it believes to be an unsafe edit. Please try a \
                 different edit.\n\n\
                 {e}"
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
        let mut parser = self.language.tree_sitter_parser().unwrap();
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
