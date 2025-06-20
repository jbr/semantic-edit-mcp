use std::collections::BTreeSet;

use super::selector::Position::{After, Around, Before, Replace};
use crate::languages::LanguageCommon;
use crate::operations::selector::NodeSelector;
use crate::tools::ExecutionResult;
use crate::validation::ContextValidator;
use anyhow::{anyhow, Result};
use diffy::{DiffOptions, PatchFormatter};
use tree_sitter::{Parser, Tree};

#[derive(Debug, Clone, serde::Serialize)]
pub struct EditOperation {
    pub(crate) target: NodeSelector,
    pub(crate) content: Option<String>,
}

#[derive(Debug)]
pub struct EditResult {
    pub message: String,
    pub new_content: String,
}

macro_rules! maybe_early_return {
    ($expr:expr) => {
        if let Some(response) = $expr {
            return Ok(ExecutionResult::ResponseOnly(response));
        }
    };
}

impl EditOperation {
    /// Get the target selector for this operation
    pub fn target_selector(&self) -> &NodeSelector {
        &self.target
    }

    /// Get a human-readable operation name
    pub fn operation_name(&self) -> &str {
        match (&self.target.position, &self.content) {
            (None, _) => "Explore",
            (Some(Before), _) => "Insert before",
            (Some(After), _) => "Insert after",
            (Some(Around), _) => "Insert around",
            (Some(Replace), Some(_)) => "Replace",
            (Some(Replace), None) => "Delete",
        }
    }

    /// Apply operation with full validation pipeline
    pub fn apply(
        &self,
        language: &LanguageCommon,
        file_path: &str,
        preview_only: bool,
    ) -> Result<ExecutionResult> {
        let source_code = std::fs::read_to_string(file_path)?;
        let mut parser = language.tree_sitter_parser()?;
        let tree = parser
            .parse(&source_code, None)
            .ok_or_else(|| anyhow!("Unable to parse {file_path} as {}", language.name()))?;

        maybe_early_return!(
            validate_tree(language, &tree, &source_code).map(|errors| format!(
                "Syntax error found prior to edit, not attempting.
Suggestion: Pause and show your human collaborator this context:\n\n{errors}"
            ))
        );

        let target_node = match self
            .target_selector()
            .find_node_with_suggestions(&tree, &source_code)
        {
            Ok(target_node) => target_node,
            Err(response) => return Ok(ExecutionResult::ResponseOnly(response)),
        };

        // Apply operation
        let editor = language.editor();
        let mut edit_result = editor.apply_operation(target_node, &tree, self, &source_code)?;

        maybe_early_return!(validate(&edit_result, &mut parser, language, &source_code)?);

        edit_result.new_content = language.editor().format_code(&edit_result.new_content)?;

        // Format response
        if preview_only {
            // Generate contextual preview showing insertion point
            return self
                .generate_contextual_preview(&edit_result, &source_code)
                .map(ExecutionResult::ResponseOnly);
        }

        let diff = generate_diff(
            &source_code,
            &edit_result.new_content,
            self.content.as_deref(),
        );

        let response = format!(
            "{} operation result:\n{}\n\n{diff}",
            self.operation_name(),
            edit_result.message,
        );
        Ok(ExecutionResult::Change {
            response,
            output: edit_result.new_content,
            output_path: file_path.to_string(),
        })
    }

    /// Generate contextual preview showing changes using diff format
    fn generate_contextual_preview(
        &self,
        result: &EditResult,
        source_code: &str,
    ) -> Result<String> {
        let new_content = &result.new_content;
        let mut preview = String::new();

        preview.push_str(&format!("STAGED: {}\n\n", self.operation_name()));
        preview.push_str(&generate_diff(
            source_code,
            new_content,
            self.content.as_deref(),
        ));

        Ok(preview)
    }

    pub(crate) fn target_selector_mut(&mut self) -> &mut NodeSelector {
        &mut self.target
    }
}

fn changed_lines(patch: &diffy::Patch<'_, str>, total_original_lines: usize) -> usize {
    let mut changed_line_numbers = BTreeSet::new();

    for hunk in patch.hunks() {
        // old_range().range() returns a std::ops::Range<usize> that's properly 0-indexed
        for line_num in hunk.old_range().range() {
            if line_num < total_original_lines {
                changed_line_numbers.insert(line_num);
            }
        }
    }
    changed_line_numbers.len()
}

fn generate_diff(source_code: &str, new_content: &str, content_patch: Option<&str>) -> String {
    let diff_patch = DiffOptions::new().create_patch(source_code, new_content);
    let formatter = PatchFormatter::new().missing_newline_message(false);

    // Get the diff string and clean it up for AI consumption
    let diff_output = formatter.fmt_patch(&diff_patch).to_string();
    let lines: Vec<&str> = diff_output.lines().collect();
    let mut cleaned_diff = String::new();

    if let Some(content_patch) = content_patch {
        let lines = content_patch.lines().count();
        if lines > 10 {
            let changed_lines = changed_lines(&diff_patch, lines);

            let changed_fraction = (changed_lines * 100) / lines;

            cleaned_diff.push_str(&format!("Edit efficiency: {changed_fraction}%\n",));
            if changed_fraction < 30 {
                cleaned_diff.push_str("💡 TIP: For focused changes like this, you might try targeted insert/replace operations for easier review and iteration\n");
            };
            cleaned_diff.push('\n');
        }
    }

    cleaned_diff.push_str("===DIFF===\n");
    for line in lines {
        // Skip ALL diff headers: file headers, hunk headers (line numbers), and any metadata
        if line.starts_with("---") || line.starts_with("+++") || line.starts_with("@@")
        // Skip "\ No newline at end of file" messages
        {
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

fn validate_tree(language: &LanguageCommon, tree: &Tree, content: &str) -> Option<String> {
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
fn validate(
    edit_result: &EditResult,
    parser: &mut Parser,
    language: &LanguageCommon,
    source_code: &str,
) -> Result<Option<String>> {
    let new_content = &edit_result.new_content;

    let new_tree = parser
        .parse(new_content, None) // we're not incremental parsing yet
        .ok_or_else(|| anyhow!("unable to parse tree"))?;

    if let Some(errors) = validate_tree(language, &new_tree, new_content) {
        let diff = generate_diff(source_code, new_content, None);
        return Ok(Some(format!(
            "This edit would result in invalid syntax, but the file is still in a valid state. \
No change was performed.
Suggestion: Try a different change.\n
{errors}\n\n{diff}"
        )));
    }

    if let Some(query) = language.validation_query() {
        let validation_result = ContextValidator::validate_tree(&new_tree, query, new_content)?;

        if !validation_result.is_valid {
            return Ok(Some(validation_result.format_errors()));
        }
    }

    Ok(None)
}
