use crate::parsers::{detect_language_from_path, TreeSitterParser};
use anyhow::{anyhow, Result};

mod context_validator;
pub use context_validator::{ContextValidator, OperationType};

pub struct SyntaxValidator;

impl SyntaxValidator {
    pub fn validate_file(file_path: &str) -> Result<BasicValidationResult> {
        let language = detect_language_from_path(file_path)
            .ok_or_else(|| anyhow!("Unable to detect language from file path: {}", file_path))?;

        let content = std::fs::read_to_string(file_path)?;
        Self::validate_content(&content, &language)
    }

    pub fn validate_content(content: &str, language: &str) -> Result<BasicValidationResult> {
        let mut parser = TreeSitterParser::new()?;
        let tree = parser.parse(language, content)?;

        let root_node = tree.root_node();
        let has_errors = root_node.has_error();

        let mut errors = Vec::new();
        if has_errors {
            Self::collect_errors(root_node, content, &mut errors);
        }

        Ok(BasicValidationResult {
            is_valid: !has_errors,
            language: language.to_string(),
            errors,
            warnings: Vec::new(), // TODO: Add warning detection
        })
    }

    /// Validate and conditionally write file with syntax safety check
    /// Returns Ok(success_message) if valid, Ok(error_message) if invalid syntax
    pub fn validate_and_write(
        file_path: &str,
        new_content: &str,
        language: &str,
        preview_only: bool,
    ) -> Result<String> {
        // Always validate syntax first
        let validation = Self::validate_content(new_content, language)?;

        if !validation.is_valid {
            let prefix = if preview_only { "PREVIEW: " } else { "" };
            return Ok(format!(
                "{}❌ Edit would create invalid syntax and was blocked:\n{}",
                prefix,
                validation
                    .errors
                    .iter()
                    .map(|e| format!("  Line {}: {}", e.line, e.message))
                    .collect::<Vec<_>>()
                    .join("\n")
            ));
        }

        // Only write if not preview mode
        if !preview_only {
            std::fs::write(file_path, new_content)?;
        }

        let prefix = if preview_only { "PREVIEW: " } else { "" };
        Ok(format!(
            "{prefix}✅ Syntax validation passed, edit successful",
        ))
    }

    fn collect_errors(node: tree_sitter::Node, source_code: &str, errors: &mut Vec<SyntaxError>) {
        if node.is_error() {
            let start_pos = node.start_position();
            let end_pos = node.end_position();

            let _ = source_code; // TODO: use source code for something, or remove it

            errors.push(SyntaxError {
                message: "Syntax error".to_string(),
                line: start_pos.row + 1,
                column: start_pos.column + 1,
                end_line: end_pos.row + 1,
                end_column: end_pos.column + 1,
                error_type: SyntaxErrorType::ParseError,
            });
        }

        if node.is_missing() {
            let pos = node.start_position();
            errors.push(SyntaxError {
                message: format!("Missing {}", node.kind()),
                line: pos.row + 1,
                column: pos.column + 1,
                end_line: pos.row + 1,
                end_column: pos.column + 1,
                error_type: SyntaxErrorType::MissingNode,
            });
        }

        // Recursively check children
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            Self::collect_errors(child, source_code, errors);
        }
    }
}

#[derive(Debug)]
pub struct BasicValidationResult {
    pub is_valid: bool,
    pub language: String,
    pub errors: Vec<SyntaxError>,
    pub warnings: Vec<SyntaxWarning>,
}

#[derive(Debug)]
pub struct SyntaxError {
    pub message: String,
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
    pub error_type: SyntaxErrorType,
}

#[derive(Debug)]
pub struct SyntaxWarning {
    pub message: String,
    pub line: usize,
    pub column: usize,
    pub warning_type: SyntaxWarningType,
}

#[derive(Debug)]
pub enum SyntaxErrorType {
    ParseError,
    MissingNode,
    UnexpectedToken,
}

#[derive(Debug)]
pub enum SyntaxWarningType {
    UnusedVariable,
    DeadCode,
    StyleViolation,
}

impl std::fmt::Display for BasicValidationResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Validation Result for {} code:", self.language)?;

        if self.is_valid {
            writeln!(f, "✅ Syntax is valid")?;
        } else {
            writeln!(f, "❌ Syntax errors found:")?;
            for error in &self.errors {
                writeln!(
                    f,
                    "  Error at {}:{}: {}",
                    error.line, error.column, error.message
                )?;
            }
        }

        if !self.warnings.is_empty() {
            writeln!(f, "\n⚠️  Warnings:")?;
            for warning in &self.warnings {
                writeln!(
                    f,
                    "  Warning at {}:{}: {}",
                    warning.line, warning.column, warning.message
                )?;
            }
        }
        Ok(())
    }
}
