use anyhow::{anyhow, Result};

use crate::operations::selector::NodeSelector;
use crate::tools::ExecutionResult;

#[derive(Debug, Clone)]
pub enum EditOperation {
    Replace {
        target: NodeSelector,
        new_content: String,
        preview_only: Option<bool>,
    },
    InsertBefore {
        target: NodeSelector,
        content: String,
        preview_only: Option<bool>,
    },
    InsertAfter {
        target: NodeSelector,
        content: String,
        preview_only: Option<bool>,
    },
    Wrap {
        target: NodeSelector,
        wrapper_template: String,
        preview_only: Option<bool>,
    },
    Delete {
        target: NodeSelector,
        preview_only: Option<bool>,
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
            EditOperation::Replace { preview_only, .. } => preview_only.unwrap_or(false),
            EditOperation::InsertBefore { preview_only, .. } => preview_only.unwrap_or(false),
            EditOperation::InsertAfter { preview_only, .. } => preview_only.unwrap_or(false),
            EditOperation::Wrap { preview_only, .. } => preview_only.unwrap_or(false),
            EditOperation::Delete { preview_only, .. } => preview_only.unwrap_or(false),
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
    pub fn apply_with_validation(
        &self,
        language_hint: Option<String>,
        file_path: &str,
        preview_only: bool,
    ) -> Result<ExecutionResult> {
        use crate::parsers::{detect_language_from_path, TreeSitterParser};
        use crate::validation::SyntaxValidator;

        let source_code = std::fs::read_to_string(file_path)?;

        let language = language_hint
            .or_else(|| detect_language_from_path(file_path))
            .ok_or_else(|| {
                anyhow!("Unable to detect language from file path and no language hint provided")
            })?;

        // 1. Parse tree (needed for validation)
        let mut parser = TreeSitterParser::new()?;
        let tree = parser.parse(&language, &source_code)?;

        // 2. Find target node using new text-anchored selection
        let target_node = self
            .target_selector()
            .find_node_with_suggestions(&tree, &source_code, &language)?
            .ok_or_else(|| anyhow!("Target node not found"))?;

        // 3. Terrible target validation with auto-exploration
        if let Some(error) = super::validation::check_terrible_target(
            self.target_selector(),
            &target_node,
            &tree,
            &source_code,
            &language,
        )? {
            return Ok(ExecutionResult::ResponseOnly(error));
        }

        // 4. Context validation
        let validator = crate::validation::ContextValidator::new()?;
        if validator.supports_language(&language) {
            let operation_type = match self {
                EditOperation::Replace { .. } => crate::validation::OperationType::Replace,
                EditOperation::InsertBefore { .. } => {
                    crate::validation::OperationType::InsertBefore
                }
                EditOperation::InsertAfter { .. } => crate::validation::OperationType::InsertAfter,
                EditOperation::Wrap { .. } => crate::validation::OperationType::Wrap,
                EditOperation::Delete { .. } => {
                    return Err(anyhow!(
                        "Delete operation not yet supported with validation"
                    ))
                }
            };

            let validation_result = validator.validate_insertion(
                &tree,
                &source_code,
                &target_node,
                self.content(),
                &language,
                &operation_type,
            )?;

            if !validation_result.is_valid {
                let prefix = if preview_only { "PREVIEW: " } else { "" };
                return Ok(ExecutionResult::ResponseOnly(format!(
                    "{prefix}{}",
                    validation_result.format_errors()
                )));
            }
        }

        // 5. Apply operation
        let result = self.apply(&source_code, &language)?;

        // 6. Syntax validation and file writing
        if let EditResult::Success {
            message,
            new_content,
            affected_range,
        } = &result
        {
            let validation = SyntaxValidator::validate_content(new_content, &language)?;

            if !validation.is_valid {
                let prefix = if preview_only { "PREVIEW: " } else { "" };
                return Ok(ExecutionResult::ResponseOnly(format!(
                    "{prefix}❌ Edit would create invalid syntax and was blocked:\n{}",
                    validation
                        .errors
                        .iter()
                        .map(|e| format!("  Line {}: {}", e.line, e.message))
                        .collect::<Vec<_>>()
                        .join("\n")
                )));
            }
        }

        // 7. Format response
        if preview_only {
            // Generate contextual preview showing insertion point
            return self
                .generate_contextual_preview(&target_node, &source_code, &language)
                .map(ExecutionResult::ResponseOnly);
        }

        match result {
            EditResult::Success {
                message,
                new_content,
                ..
            } => {
                // Normal response for actual operations
                let validation_note = if validator.supports_language(&language) {
                    "with context validation"
                } else {
                    "syntax validation only"
                };
                let response = format!(
                    "{} operation result ({validation_note}):\n{}",
                    self.operation_name(),
                    message
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
    fn apply(&self, source_code: &str, language: &str) -> Result<EditResult> {
        // Try to use the new language registry first
        if let Ok(registry) = crate::languages::LanguageRegistry::new() {
            if let Some(lang_support) = registry.get_language(language) {
                let editor = lang_support.editor();
                return editor.apply_operation(self, source_code);
            }
        }

        // Fallback to old Rust-only logic
        match language {
            "rust" => crate::editors::rust::RustEditor::apply_operation(self, source_code),
            _ => Err(anyhow!("Unsupported language for editing: {language}")),
        }
    }

    /// Generate contextual preview showing changes using diff format
    fn generate_contextual_preview(
        &self,
        _target_node: &tree_sitter::Node<'_>,
        source_code: &str,
        language: &str,
    ) -> Result<String> {
        use diffy;

        // Apply the actual operation to get the new content
        let result = self.apply(source_code, language)?;

        if let EditResult::Success { new_content, .. } = &result {
            // Use diffy to generate a clean diff
            let patch = diffy::create_patch(source_code, new_content);

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

            // Get the diff string and clean it up for AI consumption
            let diff_output = patch.to_string();
            let lines: Vec<&str> = diff_output.lines().collect();
            let mut cleaned_diff = String::new();

            for line in lines {
                // Skip ALL diff headers: file headers, hunk headers (line numbers), and any metadata
                if line.starts_with("---")
                    || line.starts_with("+++")
                    || line.starts_with("@@")
                    || line.starts_with("\\")
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

            preview.push_str(&cleaned_diff);

            Ok(preview)
        } else {
            Ok("🔍 **PREVIEW**: Operation did not produce new content".to_string())
        }
    }
}
