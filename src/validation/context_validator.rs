use crate::parsers::TreeSitterParser;
use anyhow::{Result, anyhow};
use std::collections::HashMap;
use tree_sitter::{Language, Node, Query, QueryCursor, StreamingIterator, Tree};

/// Tree-sitter based context validator for semantic code editing
pub struct ContextValidator {
    validation_queries: HashMap<String, Query>,
    language_objects: HashMap<String, Language>,
}

#[derive(Debug)]
pub struct ValidationResult {
    pub is_valid: bool,
    pub violations: Vec<ContextViolation>,
    pub can_auto_correct: bool,
}

#[derive(Debug)]
pub struct ContextViolation {
    pub violation_type: String, // "function.in.struct.fields", etc.
    pub node_type: String,      // "function_item", "struct_item", etc.
    pub location: String,       // "line:column"
    pub message: String,        // Human-readable error
    pub suggestion: ViolationSuggestion,
}

#[derive(Debug)]
pub struct ViolationSuggestion {
    pub message: String,
    pub auto_correctable: bool,
    pub corrected_operation: Option<CorrectedOperation>,
}

#[derive(Debug)]
pub struct CorrectedOperation {
    pub operation: OperationType,
    pub explanation: String,
}

#[derive(Debug)]
pub enum OperationType {
    InsertAfter,
    InsertBefore,
    InsertAfterStruct,
    InsertAfterEnum,
    InsertAfterImpl,
    InsertAfterFunction,
    InsertInModule,
    Replace,
    Wrap,
}

impl ContextValidator {
    pub fn new() -> Result<Self> {
        let mut validation_queries = HashMap::new();
        let mut language_objects = HashMap::new();

        // Initialize Rust support
        let rust_lang = tree_sitter_rust::LANGUAGE.into();
        let rust_query = Self::load_validation_query("rust", &rust_lang)?;

        language_objects.insert("rust".to_string(), rust_lang);
        validation_queries.insert("rust".to_string(), rust_query);

        Ok(Self {
            validation_queries,
            language_objects,
        })
    }

    fn load_validation_query(language: &str, lang_obj: &Language) -> Result<Query> {
        // Try to load from file first, then fall back to built-in
        let query_source =
            match std::fs::read_to_string(format!("queries/{language}/validation.scm")) {
                Ok(content) => content,
                Err(_) => Self::default_validation_query(language),
            };

        Query::new(lang_obj, &query_source)
            .map_err(|e| anyhow!("Failed to compile {} validation query: {}", language, e))
    }

    fn default_validation_query(language: &str) -> String {
        match language {
            "rust" => r#"
;; Simple fallback validation for Rust
(function_item 
  body: (block
    [(struct_item) (enum_item) (union_item)] @invalid.type.in.function.body))

(function_item
  body: (block
    (impl_item) @invalid.impl.in.function.body))

(function_item
  body: (block
    (trait_item) @invalid.trait.in.function.body))
"#
            .to_string(),
            _ => "".to_string(),
        }
    }

    /// Validate if content can be safely inserted at the target location
    pub fn validate_insertion(
        &self,
        _tree: &Tree,
        source_code: &str,
        target_node: &Node,
        content: &str,
        language: &str,
        operation_type: &OperationType,
    ) -> Result<ValidationResult> {
        let query = self
            .validation_queries
            .get(language)
            .ok_or_else(|| anyhow!("No validation rules for language: {}", language))?;

        // Simulate the insertion to create a temporary tree
        let temp_content =
            self.simulate_insertion(source_code, target_node, content, operation_type)?;

        // Parse the temporary content
        let mut temp_parser = TreeSitterParser::new()?;
        let temp_tree = temp_parser.parse(language, &temp_content)?;

        // Run validation queries against the temporary tree
        let mut cursor = QueryCursor::new();
        let mut matches = cursor.matches(query, temp_tree.root_node(), temp_content.as_bytes());

        let mut violations = Vec::new();

        while let Some(m) = matches.next() {
            for capture in m.captures {
                let node = capture.node;

                // Extract violation type from capture name
                if let Some(violation_type) = self.extract_violation_type(capture.index, query) {
                    // Only process "invalid" captures
                    if violation_type.starts_with("invalid.") {
                        violations.push(ContextViolation {
                            violation_type: violation_type.clone(),
                            node_type: node.kind().to_string(),
                            location: format!(
                                "{}:{}",
                                node.start_position().row + 1,
                                node.start_position().column + 1
                            ),
                            message: self.get_violation_message(&violation_type),
                            suggestion: self.get_violation_suggestion(
                                &violation_type,
                                &node,
                                source_code,
                            ),
                        });
                    }
                }
            }
        }

        Ok(ValidationResult {
            is_valid: violations.is_empty(),
            can_auto_correct: violations.iter().any(|v| v.suggestion.auto_correctable),
            violations,
        })
    }

    fn simulate_insertion(
        &self,
        source_code: &str,
        target_node: &Node,
        content: &str,
        operation_type: &OperationType,
    ) -> Result<String> {
        use ropey::Rope;

        let rope = Rope::from_str(source_code);
        let mut temp_rope = rope.clone();

        match operation_type {
            OperationType::InsertAfter => {
                let end_char = rope.byte_to_char(target_node.end_byte());
                temp_rope.insert(end_char, &format!("\n{content}"));
            }
            OperationType::InsertBefore => {
                let start_char = rope.byte_to_char(target_node.start_byte());
                temp_rope.insert(start_char, &format!("{content}\n"));
            }
            OperationType::Replace => {
                let start_char = rope.byte_to_char(target_node.start_byte());
                let end_char = rope.byte_to_char(target_node.end_byte());
                temp_rope.remove(start_char..end_char);
                temp_rope.insert(start_char, content);
            }
            OperationType::Wrap => {
                // For now, simulate as replacement
                let start_char = rope.byte_to_char(target_node.start_byte());
                let end_char = rope.byte_to_char(target_node.end_byte());
                let original_content = rope.slice(start_char..end_char).to_string();
                let wrapped_content = content.replace("{{content}}", &original_content);
                temp_rope.remove(start_char..end_char);
                temp_rope.insert(start_char, &wrapped_content);
            }
            _ => {
                // For specialized operations, simulate as insert after
                let end_char = rope.byte_to_char(target_node.end_byte());
                temp_rope.insert(end_char, &format!("\n{content}"));
            }
        }

        Ok(temp_rope.to_string())
    }

    fn extract_violation_type(&self, capture_index: u32, query: &Query) -> Option<String> {
        query
            .capture_names()
            .get(capture_index as usize)
            .map(|s| s.to_string())
    }

    fn get_violation_message(&self, violation_type: &str) -> String {
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

    fn get_violation_suggestion(
        &self,
        violation_type: &str,
        _node: &Node,
        _source_code: &str,
    ) -> ViolationSuggestion {
        match violation_type {
            "invalid.function.in.struct.fields" | "invalid.function.in.enum.variants" => {
                ViolationSuggestion {
                    message: "Place the function after the type definition".to_string(),
                    auto_correctable: true,
                    corrected_operation: Some(CorrectedOperation {
                        operation: OperationType::InsertAfterStruct,
                        explanation: "Moving function to after struct/enum definition".to_string(),
                    }),
                }
            }
            "invalid.type.in.function.body"
            | "invalid.impl.in.function.body"
            | "invalid.trait.in.function.body" => ViolationSuggestion {
                message: "Move this to module level".to_string(),
                auto_correctable: true,
                corrected_operation: Some(CorrectedOperation {
                    operation: OperationType::InsertInModule,
                    explanation: "Moving definition to module level".to_string(),
                }),
            },
            "invalid.use.in.item.body" => ViolationSuggestion {
                message: "Move use declarations to the top of the file".to_string(),
                auto_correctable: true,
                corrected_operation: Some(CorrectedOperation {
                    operation: OperationType::InsertInModule,
                    explanation: "Moving use declaration to module level".to_string(),
                }),
            },
            _ => ViolationSuggestion {
                message: "Consider placing this construct in an appropriate context".to_string(),
                auto_correctable: false,
                corrected_operation: None,
            },
        }
    }
}

impl ValidationResult {
    pub fn format_errors(&self) -> String {
        if self.is_valid {
            return "✅ All validations passed".to_string();
        }

        let mut response = String::new();
        response.push_str("❌ Invalid placement detected:\n\n");

        for violation in &self.violations {
            response.push_str(&format!(
                "• {} at {}: {}\n",
                violation.node_type, violation.location, violation.message
            ));

            if violation.suggestion.auto_correctable {
                if let Some(correction) = &violation.suggestion.corrected_operation {
                    response.push_str(&format!(
                        "  💡 Auto-correction available: Use {:?} operation instead.\n",
                        correction.operation
                    ));
                    response.push_str(&format!("     {}\n", correction.explanation));
                }
            } else {
                response.push_str(&format!(
                    "  💡 Suggestion: {}\n",
                    violation.suggestion.message
                ));
            }
        }

        response
    }
}
