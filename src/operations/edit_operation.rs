use std::collections::BTreeSet;

use crate::languages::utils::collect_errors;
use crate::languages::LanguageCommon;
use crate::operations::selector::NodeSelector;
use crate::tools::ExecutionResult;
use crate::validation::ContextValidator;
use anyhow::{anyhow, Result};
use diffy::{DiffOptions, PatchFormatter};
use tree_sitter::{Node, Parser, Tree};

#[derive(Debug, Clone)]
pub enum EditOperation {
    Replace {
        target: NodeSelector,
        new_content: String,
        preview_only: bool,
    },
    InsertBefore {
        target: NodeSelector,
        content: String,
        preview_only: bool,
    },
    InsertAfter {
        target: NodeSelector,
        content: String,
        preview_only: bool,
    },
    Wrap {
        target: NodeSelector,
        wrapper_template: String,
        preview_only: bool,
    },
    Delete {
        target: NodeSelector,
        preview_only: bool,
    },
}

#[derive(Debug)]
pub enum EditResult {
    Success {
        message: String,
        new_content: String,
    },
    Error(String),
}

impl EditResult {
    pub(crate) fn set_message(&mut self, new_message: String) {
        match self {
            EditResult::Success { message, .. } => *message = new_message,
            EditResult::Error(message) => *message = new_message,
        }
    }

    pub(crate) fn message(&self) -> &str {
        match self {
            EditResult::Success { message, .. } => message,
            EditResult::Error(message) => message,
        }
    }
}

macro_rules! maybe_early_return {
    ($expr:expr) => {
        if let Some(response) = $expr {
            return Ok(ExecutionResult::ResponseOnly(response));
        }
    };
}

impl EditOperation {
    pub fn is_preview_only(&self) -> bool {
        match self {
            EditOperation::Replace { preview_only, .. } => *preview_only,
            EditOperation::InsertBefore { preview_only, .. } => *preview_only,
            EditOperation::InsertAfter { preview_only, .. } => *preview_only,
            EditOperation::Wrap { preview_only, .. } => *preview_only,
            EditOperation::Delete { preview_only, .. } => *preview_only,
        }
    }

    /// Get the target selector for this operation
    pub fn target_selector(&self) -> &NodeSelector {
        match self {
            EditOperation::Replace { target, .. } => target,
            EditOperation::InsertBefore { target, .. } => target,
            EditOperation::InsertAfter { target, .. } => target,
            EditOperation::Wrap { target, .. } => target,
            EditOperation::Delete { target, .. } => target,
        }
    }

    /// Get a human-readable operation name
    pub fn operation_name(&self) -> &str {
        match self {
            EditOperation::Replace { .. } => "Replace",
            EditOperation::InsertBefore { .. } => "Insert before",
            EditOperation::InsertAfter { .. } => "Insert after",
            EditOperation::Wrap { .. } => "Wrap",
            EditOperation::Delete { .. } => "Delete",
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
            .ok_or_else(|| anyhow!("failed to parse {}", language.name()))?;

        maybe_early_return!(
            validate_tree(language, &tree, &source_code).map(|errors| format!(
                "Syntax error found prior to edit, not attempting.
Suggestion: Pause and show your human collaborator this context:\n\n{errors}"
            ))
        );

        // Find target node using new text-anchored selection
        let target_node = match self
            .target_selector()
            .find_node_with_suggestions(&tree, &source_code)
        {
            Ok(target_node) => target_node,
            Err(response) => return Ok(ExecutionResult::ResponseOnly(response)),
        };

        // Apply operation
        let mut edit_result = self.apply_inner(target_node, &tree, &source_code, language)?;

        maybe_early_return!(validate(
            &edit_result,
            &mut parser,
            language,
            &source_code,
            &tree
        )?);

        if let EditResult::Success { new_content, .. } = &mut edit_result {
            *new_content = language.editor().format_code(new_content)?;
        }

        // Format response
        if preview_only {
            // Generate contextual preview showing insertion point
            return self
                .generate_contextual_preview(&edit_result, &source_code)
                .map(ExecutionResult::ResponseOnly);
        }

        match edit_result {
            EditResult::Success {
                message,
                new_content,
                ..
            } => {
                let diff = generate_diff(&source_code, &new_content);

                let response = format!(
                    "{} operation result:\n{message}\n\n{diff}",
                    self.operation_name(),
                );
                Ok(ExecutionResult::Change {
                    response,
                    output: new_content,
                    output_path: file_path.to_string(),
                })
            }

            EditResult::Error(message) => Ok(ExecutionResult::ResponseOnly(message)),
        }
    }

    /// Apply the edit operation to source code
    fn apply_inner<'tree>(
        &self,
        target_node: Node<'tree>,
        tree: &Tree,
        source_code: &str,
        language: &LanguageCommon,
    ) -> Result<EditResult> {
        let editor = language.editor();
        let mut edit_result = editor.apply_operation(target_node, tree, self, source_code)?;

        if self.is_preview_only() {
            edit_result.set_message(format!("PREVIEW: {}", edit_result.message()));
        }
        Ok(edit_result)
    }

    /// Generate contextual preview showing changes using diff format
    fn generate_contextual_preview(
        &self,
        result: &EditResult,
        source_code: &str,
    ) -> Result<String> {
        if let EditResult::Success { new_content, .. } = &result {
            let mut preview = String::new();

            // Add operation-specific header
            match self {
                EditOperation::Replace { .. } => {
                    preview.push_str("🔍 **REPLACEMENT PREVIEW**\n\n");
                }
                EditOperation::InsertBefore { .. } | EditOperation::InsertAfter { .. } => {
                    preview.push_str("🔍 **INSERTION PREVIEW**\n\n");
                }
                EditOperation::Wrap { .. } => {
                    preview.push_str("🔍 **WRAP PREVIEW**\n\n");
                }
                EditOperation::Delete { .. } => {
                    preview.push_str("🔍 **DELETE PREVIEW**\n\n");
                }
            }

            preview.push_str(&generate_diff(source_code, new_content));

            Ok(preview)
        } else {
            Ok("🔍 **PREVIEW**: Operation did not produce new content".to_string())
        }
    }
}

fn generate_diff(source_code: &str, new_content: &str) -> String {
    let patch = DiffOptions::new().create_patch(source_code, new_content);
    let formatter = PatchFormatter::new().missing_newline_message(false);

    // Get the diff string and clean it up for AI consumption
    let diff_output = formatter.fmt_patch(&patch).to_string();
    let lines: Vec<&str> = diff_output.lines().collect();
    let mut cleaned_diff = String::from("===DIFF===\n");

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
    tree: &Tree,
) -> Result<Option<String>> {
    let EditResult::Success { new_content, .. } = &edit_result else {
        return Ok(None);
    };

    let old_tree = if language.name() == "markdown" {
        // workaround for a segfault in markdown
        None
    } else {
        Some(tree)
    };

    let new_tree = parser
        .parse(new_content, old_tree)
        .ok_or_else(|| anyhow!("unable to parse tree"))?;

    if let Some(errors) = validate_tree(language, &new_tree, new_content) {
        let diff = generate_diff(source_code, new_content);
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
