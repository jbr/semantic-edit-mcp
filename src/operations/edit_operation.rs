use std::borrow::Cow;

use crate::languages::LanguageSupport;
use crate::operations::selector::NodeSelector;
use crate::tools::ExecutionResult;
use crate::validation::{ContextValidator, OperationType, SyntaxValidator, ValidationResult};
use crate::{languages::LanguageRegistry, validation};
use anyhow::{anyhow, Result};
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
        affected_range: (usize, usize),
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

    /// Get the content for this operation
    pub fn content(&self) -> &str {
        match self {
            EditOperation::Replace { new_content, .. } => new_content,
            EditOperation::InsertBefore { content, .. } => content,
            EditOperation::InsertAfter { content, .. } => content,
            EditOperation::Wrap {
                wrapper_template, ..
            } => wrapper_template,
            EditOperation::Delete { .. } => "",
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
        language: &dyn LanguageSupport,
        file_path: &str,
        preview_only: bool,
    ) -> Result<ExecutionResult> {
        let source_code = std::fs::read_to_string(file_path)?;
        let mut parser = language.tree_sitter_parser();
        let tree = parser
            .parse(&source_code, None)
            .ok_or_else(|| anyhow!("failed to parse {}", language.language_name()))?;

        // Find target node using new text-anchored selection
        let target_node = self
            .target_selector()
            .find_node_with_suggestions(&tree, &source_code, language.language_name())?
            .ok_or_else(|| anyhow!("Target node not found"))?;

        // Apply operation
        let edit_result = self.apply_inner(target_node, &tree, &source_code, language)?;

        if let Some(invalid_syntax_response) =
            self.validate_syntax(&tree, &edit_result, preview_only, language)?
        {
            return Ok(invalid_syntax_response);
        }

        if let Some(invalid_context_response) =
            self.validate_context(&tree, &edit_result, preview_only, language)?
        {
            return Ok(invalid_context_response);
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
                    "{} operation result:\n{message}\n\n===DIFF===\n{diff}",
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
        language: &dyn LanguageSupport,
    ) -> Result<EditResult> {
        let editor = language.editor();
        let mut edit_result = editor.apply_operation(target_node, tree, self, source_code)?;
        if self.is_preview_only() {
            edit_result.set_message(format!("PREVIEW: {}", edit_result.message()));
        }
        return Ok(edit_result);
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
                    preview.push_str("🔍 **REPLACEMENT PREVIEW** - Changes to be made:\n\n");
                }
                EditOperation::InsertBefore { .. } | EditOperation::InsertAfter { .. } => {
                    preview.push_str("🔍 **INSERTION PREVIEW** - Changes to be made:\n\n");
                }
                EditOperation::Wrap { .. } => {
                    preview.push_str("🔍 **WRAP PREVIEW** - Changes to be made:\n\n");
                }
                EditOperation::Delete { .. } => {
                    preview.push_str("🔍 **DELETE PREVIEW** - Changes to be made:\n\n");
                }
            }

            preview.push_str(&generate_diff(source_code, new_content));

            Ok(preview)
        } else {
            Ok("🔍 **PREVIEW**: Operation did not produce new content".to_string())
        }
    }

    fn validate_syntax(
        &self,
        tree: &Tree,
        edit_result: &EditResult,
        preview_only: bool,
        language: &dyn LanguageSupport,
    ) -> Result<Option<ExecutionResult>> {
        let EditResult::Success { new_content, .. } = edit_result else {
            return Ok(None);
        };
        let validation = SyntaxValidator::validate_content(&tree, new_content, language)?;

        if !validation.is_valid {
            let prefix = if preview_only { "PREVIEW: " } else { "" };
            return Ok(Some(ExecutionResult::ResponseOnly(format!(
                "{prefix}❌ Edit would create invalid syntax and was blocked:\n{}",
                validation
                    .errors
                    .iter()
                    .map(|e| format!("  Line {}: {}", e.line, e.message))
                    .collect::<Vec<_>>()
                    .join("\n")
            ))));
        }

        Ok(None)
    }

    fn validate_context(
        &self,
        tree: &Tree,
        edit_result: &EditResult,
        preview_only: bool,
        language: &dyn LanguageSupport,
    ) -> Result<Option<ExecutionResult>> {
        let EditResult::Success { new_content, .. } = edit_result else {
            return Ok(None);
        };

        let language_queries = language.load_queries()?;
        if let Some(query) = language_queries.validation_queries {
            let validation_result =
                ContextValidator::validate_tree(language, tree, &query, new_content)?;

            if !validation_result.is_valid {
                let prefix = if preview_only { "PREVIEW: " } else { "" };
                return Ok(Some(ExecutionResult::ResponseOnly(format!(
                    "{prefix}{}",
                    validation_result.format_errors()
                ))));
            }
        }
        Ok(None)
    }
}

fn generate_diff(source_code: &str, new_content: &str) -> String {
    // Use diffy to generate a clean diff

    let patch = diffy::DiffOptions::new().create_patch(source_code, new_content);
    let formatter = diffy::PatchFormatter::new().missing_newline_message(false);

    // Get the diff string and clean it up for AI consumption
    let diff_output = formatter.fmt_patch(&patch).to_string();
    let lines: Vec<&str> = diff_output.lines().collect();
    let mut cleaned_diff = String::new();

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
