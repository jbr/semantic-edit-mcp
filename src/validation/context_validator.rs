use anyhow::Result;
use tree_sitter::{Node, Query, QueryCursor, StreamingIterator, Tree};

/// Tree-sitter based context validator for semantic code editing
pub struct ContextValidator;

#[derive(Debug)]
pub struct ValidationResult<'tree, 'source> {
    pub is_valid: bool,
    pub violations: Vec<ContextViolation<'tree>>,
    pub source_code: &'source str,
}

#[derive(Debug)]
pub struct ContextViolation<'tree> {
    pub node: Node<'tree>,
    pub message: String, // Human-readable error
    pub suggestion: &'static str,
}

impl ContextValidator {
    /// Validate if content can be safely inserted at the target location
    pub fn validate_tree<'tree, 'source>(
        tree: &'tree Tree,
        query: &Query,
        source_code: &'source str,
    ) -> Result<ValidationResult<'tree, 'source>> {
        // Run validation queries against the temporary tree
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(query, tree.root_node(), source_code.as_bytes());

        let mut violations = Vec::new();

        while let Some(m) = matches.next() {
            for capture in m.captures {
                let node = capture.node;

                // Extract violation type from capture name
                if let Some(violation_type) = Self::extract_violation_type(capture.index, query) {
                    // Only process "invalid" captures
                    if violation_type.starts_with("invalid.") {
                        violations.push(ContextViolation {
                            node,
                            message: Self::get_violation_message(&violation_type),
                            suggestion: Self::get_violation_suggestion(&violation_type),
                        });
                    }
                }
            }
        }

        Ok(ValidationResult {
            is_valid: violations.is_empty(),
            source_code,
            violations,
        })
    }

    fn extract_violation_type(capture_index: u32, query: &Query) -> Option<String> {
        query
            .capture_names()
            .get(capture_index as usize)
            .map(|s| s.to_string())
    }

    fn get_violation_message(violation_type: &str) -> String {
        match violation_type {
            "invalid.function.in.struct.fields" => {
                "Functions cannot be defined inside struct field lists".to_string()
            }
            "invalid.function.in.enum.variants" => {
                "Functions cannot be defined inside enum variant lists".to_string()
            }
            "invalid.type.in.function.body" => {
                "Type definitions cannot be placed inside function bodies".to_string()
            }
            "invalid.impl.in.function.body" => {
                "Impl blocks cannot be placed inside function bodies".to_string()
            }
            "invalid.trait.in.function.body" => {
                "Trait definitions cannot be placed inside function bodies".to_string()
            }
            "invalid.impl.nested" => "Impl blocks can only be defined at module level".to_string(),
            "invalid.trait.nested" => {
                "Trait definitions can only be defined at module level".to_string()
            }
            "invalid.use.in.item.body" => "Use declarations should be at module level".to_string(),
            "invalid.const.in.function.body" => {
                "Const/static items should be at module level".to_string()
            }
            "invalid.mod.in.function.body" => {
                "Module declarations cannot be inside function bodies".to_string()
            }
            "invalid.item.nested.in.item" => {
                "Items cannot be nested inside other items".to_string()
            }
            "invalid.expression.as.type" => "Expressions cannot be used as types".to_string(),
            _ => format!(
                "Invalid placement: {}",
                violation_type
                    .strip_prefix("invalid.")
                    .unwrap_or(violation_type)
            ),
        }
    }

    fn get_violation_suggestion(violation_type: &str) -> &'static str {
        match violation_type {
            "invalid.function.in.struct.fields" | "invalid.function.in.enum.variants" => {
                "Place the function after the type definition"
            }
            "invalid.type.in.function.body"
            | "invalid.impl.in.function.body"
            | "invalid.trait.in.function.body" => "Move this to module level",

            "invalid.use.in.item.body" => "Move use declarations to the top of the file",
            _ => "Consider placing this construct in an appropriate context",
        }
    }
}

impl ValidationResult<'_, '_> {
    pub fn format_errors(&self) -> String {
        if self.is_valid {
            return "✅ All validations passed".to_string();
        }

        let mut response = String::new();
        response.push_str("❌ Invalid placement detected:\n\n");

        for violation in &self.violations {
            response.push_str(&format!("• {}:\n", violation.message));
            let parent = violation.node.parent().unwrap_or(violation.node);
            response.push_str(&self.source_code[parent.byte_range()]);
            response.push_str("\n\n");
            response.push_str(&format!("  💡 Suggestion: {}\n", violation.suggestion));
        }

        response
    }
}
