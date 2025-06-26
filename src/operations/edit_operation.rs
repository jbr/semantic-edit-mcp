use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use super::selector::{Selector, TextRange};
use crate::validation::ContextValidator;
use crate::{languages::LanguageCommon, state::StagedOperation};
use anyhow::{anyhow, Result};
use diffy::{DiffOptions, PatchFormatter};
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use tree_sitter::{Parser, Tree};

#[derive(Debug)]
pub enum ExecutionResult {
    ResponseOnly(String),
    ChangeStaged(String, StagedOperation),
    Change {
        response: String,
        output: String,
        output_path: PathBuf,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct EditOperation {
    /// How to position the `content`
    #[serde(deserialize_with = "deserialize_selector")]
    pub selector: Selector,
    /// The new content to insert or replace
    pub content: String,
}

fn deserialize_selector<'de, D>(deserializer: D) -> Result<Selector, D::Error>
where
    D: Deserializer<'de>,
{
    let value: Value = Deserialize::deserialize(deserializer)?;
    match value {
        Value::String(s) => serde_json::from_str(&s).map_err(serde::de::Error::custom),
        _ => Selector::deserialize(value).map_err(serde::de::Error::custom),
    }
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
    /// Create a new edit operation
    pub fn new(selector: Selector, content: String) -> Self {
        Self { selector, content }
    }

    /// Get a human-readable operation name
    pub fn operation_name(&self) -> &str {
        self.selector.operation_name()
    }

    /// Get the content for this operation
    pub fn content(&self) -> &str {
        &self.content
    }

    /// Get a mutable reference to the selector for retargeting
    pub fn selector_mut(&mut self) -> &mut Selector {
        &mut self.selector
    }

    /// Replace the selector while keeping the same content (for retargeting)
    pub fn retarget(&mut self, selector: Selector) {
        self.selector = selector;
    }

    /// Apply operation with full validation pipeline
    pub fn apply(
        &self,
        language: &LanguageCommon,
        file_path: &Path,
        preview_only: bool,
    ) -> Result<ExecutionResult> {
        log::trace!(
            "top of apply for {self:?} and language {language} with {}",
            file_path.display()
        );

        let source_code = std::fs::read_to_string(file_path)?;
        let mut parser = language.tree_sitter_parser()?;
        let tree = parser.parse(&source_code, None).ok_or_else(|| {
            anyhow!(
                "Unable to parse {} as {}",
                file_path.display(),
                language.name()
            )
        })?;

        maybe_early_return!(
            validate_tree(language, &tree, &source_code).map(|errors| format!(
                "Syntax error found prior to edit, not attempting.
Suggestion: Pause and show your human collaborator this context:\n\n{errors}"
            ))
        );

        // Find text ranges for the operation
        let text_ranges = match self.selector.find_text_ranges(&source_code) {
            Ok(ranges) => ranges,
            Err(response) => return Ok(ExecutionResult::ResponseOnly(response)),
        };

        // Handle disambiguation if multiple ranges found
        if text_ranges.len() > 1 {
            return Ok(ExecutionResult::ResponseOnly(format_multiple_matches(
                &text_ranges,
                &source_code,
            )));
        }

        if text_ranges.is_empty() {
            return Ok(ExecutionResult::ResponseOnly(
                "No valid text ranges found".to_string(),
            ));
        }

        // Apply the edit using the first (and only) range
        let text_range = &text_ranges[0];
        let new_content = text_range.apply_edit(&source_code, &self.content);

        let edit_result = EditResult {
            message: format!("Applied {} operation", self.operation_name()),
            new_content,
        };

        maybe_early_return!(validate(&edit_result, &mut parser, language, &source_code)?);

        let formatted_content = language.editor().format_code(&edit_result.new_content)?;
        let final_result = EditResult {
            message: edit_result.message,
            new_content: formatted_content,
        };

        // Format response
        if preview_only {
            return self
                .generate_contextual_preview(&final_result, &source_code)
                .map(ExecutionResult::ResponseOnly);
        }

        let diff = generate_diff(&source_code, &final_result.new_content, Some(&self.content));

        let response = format!(
            "{} operation result:\n{}\n\n{diff}",
            self.operation_name(),
            final_result.message,
        );
        Ok(ExecutionResult::Change {
            response,
            output: final_result.new_content,
            output_path: file_path.to_path_buf(),
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
            Some(&self.content),
        ));

        Ok(preview)
    }
}

fn format_multiple_matches(ranges: &[TextRange], source_code: &str) -> String {
    let mut message = format!(
        "Found {} possible matches. Please be more specific:\n\n",
        ranges.len()
    );

    for (i, range) in ranges.iter().enumerate() {
        match range {
            TextRange::Insert(insert) => {
                let context = get_context_around_byte_position(source_code, insert.byte_offset, 50);
                message.push_str(&format!(
                    "{}. Insert {} anchor \"{}\": {}\n",
                    i + 1,
                    match insert.position {
                        super::selector::InsertPosition::Before => "before",
                        super::selector::InsertPosition::After => "after",
                    },
                    insert.anchor,
                    context
                ));
            }
            TextRange::Replace(replace) => {
                let text = &source_code[replace.start_byte..replace.end_byte];
                let preview = if text.len() > 100 {
                    format!("{}...", &text[..97])
                } else {
                    text.to_string()
                };
                message.push_str(&format!(
                    "{}. {}: {}\n",
                    i + 1,
                    range.description(),
                    preview.replace('\n', "\\n")
                ));
            }
        }
    }

    message.push_str(
        "\nSuggestion: Add more context to your anchor text to uniquely identify the target.",
    );
    message
}

fn get_context_around_byte_position(
    source_code: &str,
    byte_pos: usize,
    context_chars: usize,
) -> String {
    let start = byte_pos.saturating_sub(context_chars);
    let end = (byte_pos + context_chars).min(source_code.len());
    source_code[start..end]
        .replace('\n', "\\n")
        .replace('\t', "\\t")
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::operations::selector::{InsertPosition, Selector};

    #[test]
    fn test_retargeting() {
        let mut operation = EditOperation::new(
            Selector::Insert {
                anchor: "fn main()".to_string(),
                position: InsertPosition::Before,
            },
            "// Comment\n".to_string(),
        );

        // Original target
        assert_eq!(operation.operation_name(), "Insert");

        // Retarget to a different location
        operation.retarget(Selector::Insert {
            anchor: "fn test()".to_string(),
            position: InsertPosition::After,
        });

        // Content stays the same, but selector changes
        assert_eq!(operation.content(), "// Comment\n");
        match &operation.selector {
            Selector::Insert { anchor, position } => {
                assert_eq!(anchor, "fn test()");
                assert!(matches!(position, InsertPosition::After));
            }
            _ => panic!("Expected insert selector"),
        }
    }

    #[test]
    fn test_selector_separation() {
        // Test that we can deserialize just the selector part
        let json = r#"{"selector": {"operation": "insert", "anchor": "fn main()", "position": "before"}, "content": "test"}"#;

        let operation: EditOperation = serde_json::from_str(json).unwrap();

        // Verify the selector part
        match &operation.selector {
            Selector::Insert { anchor, position } => {
                assert_eq!(anchor, "fn main()");
                assert!(matches!(position, InsertPosition::Before));
            }
            _ => panic!("Expected insert selector"),
        }

        assert_eq!(operation.content, "test");
    }
}
