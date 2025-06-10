use anyhow::{anyhow, Result};
use diffy;
use serde::{Deserialize, Serialize};
use tree_sitter::{Node, StreamingIterator};

use crate::{
    parsers::{detect_language_from_path, rust::RustParser},
    tools::ExecutionResult,
    validation::SyntaxValidator,
};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum NodeSelector {
    #[serde(rename = "name")]
    Name {
        node_type: Option<String>,
        name: String,
    },
    #[serde(rename = "type")]
    Type { node_type: String },
    #[serde(rename = "query")]
    Query { query: String },
    #[serde(rename = "position")]
    Position {
        line: usize,
        column: usize,
        /// Optional scope hint: "token" (default), "expression", "statement", "item"
        #[serde(default)]
        scope: Option<String>,
    },
}

/// Find an ancestor node of one of the specified types
fn find_ancestor_of_type<'a>(node: &Node<'a>, target_types: &[&str]) -> Option<Node<'a>> {
    let mut current = *node;
    while let Some(parent) = current.parent() {
        if target_types.contains(&parent.kind()) {
            return Some(parent);
        }
        current = parent;
    }
    None
}

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

#[derive(Debug, Clone)]
pub struct NodeNotFoundError {
    pub selector: NodeSelector,
    pub suggestions: Vec<String>,
    pub available_options: Vec<String>,
}

impl NodeNotFoundError {
    pub fn new(selector: NodeSelector) -> Self {
        Self {
            selector,
            suggestions: Vec::new(),
            available_options: Vec::new(),
        }
    }

    pub fn with_suggestions(mut self, suggestions: Vec<String>) -> Self {
        self.suggestions = suggestions;
        self
    }

    pub fn with_available_options(mut self, available_options: Vec<String>) -> Self {
        self.available_options = available_options;
        self
    }

    pub fn to_detailed_message(&self) -> String {
        let mut message = match &self.selector {
            NodeSelector::Name { name, node_type } => {
                if let Some(nt) = node_type {
                    format!("Node '{name}' of type '{nt}' not found")
                } else {
                    format!("Node '{name}' not found")
                }
            }
            NodeSelector::Type { node_type } => {
                format!("No nodes of type '{node_type}' found")
            }
            NodeSelector::Query { query } => {
                format!("Query '{query}' returned no results")
            }
            NodeSelector::Position { line, column, .. } => {
                format!("No suitable node found at position {line}:{column}")
            }
        };

        if !self.available_options.is_empty() {
            message.push_str(&format!(
                "\n\nAvailable options: {}",
                self.available_options.join(", ")
            ));
        }

        if !self.suggestions.is_empty() {
            message.push_str(&format!(
                "\n\nDid you mean: {}",
                self.suggestions.join(", ")
            ));
        }

        message
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

    /// Apply operation with full validation pipeline (terrible target check, context validation, syntax validation)
    pub fn apply_with_validation(
        &self,
        language_hint: Option<String>,
        file_path: &str,
        preview_only: bool,
    ) -> Result<ExecutionResult> {
        let source_code = std::fs::read_to_string(file_path)?;

        let language = language_hint
            .or_else(|| detect_language_from_path(file_path))
            .ok_or_else(|| {
                anyhow!("Unable to detect language from file path and no language hint provided")
            })?;

        // 1. Parse tree (needed for validation)
        let mut parser = crate::parsers::TreeSitterParser::new()?;
        let tree = parser.parse(&language, &source_code)?;

        // 2. Find target node
        let target_node = self
            .target_selector()
            .find_node_with_suggestions(&tree, &source_code, &language)?
            .ok_or_else(|| anyhow!("Target node not found"))?;

        // 3. Terrible target validation with auto-exploration
        if let Some(error) =
            self.check_terrible_target(&target_node, &tree, &source_code, &language)?
        {
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

        // 5. Apply operation (existing logic)
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

    /// Get the target selector for this operation
    fn target_selector(&self) -> &NodeSelector {
        match self {
            EditOperation::Replace { target, .. } => target,
            EditOperation::InsertBefore { target, .. } => target,
            EditOperation::InsertAfter { target, .. } => target,
            EditOperation::Wrap { target, .. } => target,
            EditOperation::Delete { target, .. } => target,
        }
    }

    /// Get the content for this operation
    fn content(&self) -> &str {
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
    fn operation_name(&self) -> &str {
        match self {
            EditOperation::Replace { .. } => "Replace",
            EditOperation::InsertBefore { .. } => "Insert before",
            EditOperation::InsertAfter { .. } => "Insert after",
            EditOperation::Wrap { .. } => "Wrap",
            EditOperation::Delete { .. } => "Delete",
        }
    }

    /// Generate contextual preview showing changes using diff format
    fn generate_contextual_preview(
        &self,
        _target_node: &tree_sitter::Node<'_>,
        source_code: &str,
        language: &str,
    ) -> Result<String> {
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

    /// Create a version of this operation with placeholder content for preview
    fn with_placeholder_content(&self) -> Self {
        match self {
            EditOperation::Replace { target, .. } => EditOperation::Replace {
                target: target.clone(),
                new_content: "🎯 REPLACEMENT_CONTENT 🎯".to_string(),
                preview_only: Some(true),
            },
            EditOperation::InsertBefore { target, .. } => EditOperation::InsertBefore {
                target: target.clone(),
                content: "🎯 NEW_CONTENT 🎯".to_string(),
                preview_only: Some(true),
            },
            EditOperation::InsertAfter { target, .. } => EditOperation::InsertAfter {
                target: target.clone(),
                content: "🎯 NEW_CONTENT 🎯".to_string(),
                preview_only: Some(true),
            },
            EditOperation::Wrap { target, .. } => EditOperation::Wrap {
                target: target.clone(),
                wrapper_template: "🎯 WRAPPER_START 🎯{{content}}🎯 WRAPPER_END 🎯".to_string(),
                preview_only: Some(true),
            },
            EditOperation::Delete { target, .. } => EditOperation::Delete {
                target: target.clone(),
                preview_only: Some(true),
            },
        }
    }

    /// Check for structural warnings (less severe than terrible targets)
    fn check_structural_warning(
        &self,
        target_node: &tree_sitter::Node<'_>,
    ) -> Result<Option<String>> {
        let node_kind = target_node.kind();
        let parent_kind = target_node.parent().map(|p| p.kind());

        Ok(match self {
            EditOperation::InsertAfter { .. } => {
                match node_kind {
                    "impl_item" | "struct_item" | "enum_item" | "mod_item" => {
                        Some("You're inserting after a container block. Content will be placed OUTSIDE the container, not inside it.".to_string())
                    }
                    "function_item" if parent_kind == Some("impl_item") => {
                        Some("Inserting after this method will place content at module level, outside the impl block.".to_string())
                    }
                    "block" => {
                        Some("Inserting after a block will place content outside the block scope.".to_string())
                    }
                    _ => None
                }
            }
            EditOperation::Replace { .. } => {
                match node_kind {
                    "impl_item" | "struct_item" | "enum_item" => {
                        Some("You're replacing an entire container definition. This will remove all its contents.".to_string())
                    }
                    _ => None
                }
            }
            _ => None
        })
    }

    fn check_terrible_target(
        &self,
        target_node: &tree_sitter::Node<'_>,
        tree: &tree_sitter::Tree,
        source_code: &str,
        language: &str,
    ) -> Result<Option<String>> {
        check_terrible_target(
            self.target_selector(),
            target_node,
            tree,
            source_code,
            language,
        )
    }
}

impl NodeSelector {
    pub fn find_node<'a>(
        &self,
        tree: &'a tree_sitter::Tree,
        source_code: &str,
        language: &str,
    ) -> Result<Option<Node<'a>>> {
        // Try to use the new language registry first
        if let Ok(registry) = crate::languages::LanguageRegistry::new() {
            if let Some(lang_support) = registry.get_language(language) {
                let parser = lang_support.parser();

                match self {
                    NodeSelector::Position {
                        line,
                        column,
                        scope,
                    } => {
                        let node = crate::parsers::find_node_by_position(tree, *line, *column);
                        if let Some(node) = node {
                            // Apply scope-based filtering if requested
                            let final_node = match scope.as_deref() {
                                Some("expression") => find_ancestor_of_type(
                                    &node,
                                    &[
                                        "expression_statement",
                                        "call_expression",
                                        "macro_invocation",
                                    ],
                                ),
                                Some("statement") => find_ancestor_of_type(
                                    &node,
                                    &[
                                        "expression_statement",
                                        "let_declaration",
                                        "item_declaration",
                                    ],
                                ),
                                Some("item") => find_ancestor_of_type(
                                    &node,
                                    &["function_item", "struct_item", "impl_item", "mod_item"],
                                ),
                                Some("token") | None => Some(node), // Default behavior
                                _ => Some(node),                    // Unknown scope, use default
                            };
                            return Ok(final_node);
                        } else {
                            return Ok(None);
                        }
                    }
                    NodeSelector::Name { node_type, name } => {
                        if let Some(nt) = node_type {
                            // Use the new fallback-aware function
                            return RustParser::find_by_name_with_fallback(
                                tree,
                                source_code,
                                nt,
                                name,
                            );
                        } else {
                            // Try common node types for this language
                            let node_types = match language {
                                "rust" => vec!["function_item", "struct_item", "enum_item"],
                                "json" => vec!["pair", "object", "array"],
                                "toml" => vec!["table", "pair"],
                                "markdown" => vec!["atx_heading", "fenced_code_block"],
                                _ => vec!["function_item", "struct_item"], // fallback
                            };

                            for nt in node_types {
                                if let Ok(Some(node)) =
                                    parser.find_by_name(tree, source_code, nt, name)
                                {
                                    return Ok(Some(node));
                                }
                            }
                            return Ok(None);
                        }
                    }
                    NodeSelector::Type { node_type } => {
                        let nodes = parser.find_by_type(tree, node_type);
                        return Ok(nodes.into_iter().next());
                    }
                    NodeSelector::Query { query } => {
                        let nodes = parser.execute_query(query, tree, source_code)?;
                        return Ok(nodes.into_iter().next());
                    }
                }
            }
        }

        // Fallback to old Rust-only logic
        match self {
            NodeSelector::Position {
                line,
                column,
                scope,
            } => {
                let node = crate::parsers::find_node_by_position(tree, *line, *column);
                if let Some(node) = node {
                    // Apply scope-based filtering if requested
                    let final_node = match scope.as_deref() {
                        Some("expression") => find_ancestor_of_type(
                            &node,
                            &[
                                "expression_statement",
                                "call_expression",
                                "macro_invocation",
                            ],
                        ),
                        Some("statement") => find_ancestor_of_type(
                            &node,
                            &[
                                "expression_statement",
                                "let_declaration",
                                "item_declaration",
                            ],
                        ),
                        Some("item") => find_ancestor_of_type(
                            &node,
                            &["function_item", "struct_item", "impl_item", "mod_item"],
                        ),
                        Some("token") | None => Some(node), // Default behavior
                        _ => Some(node),                    // Unknown scope, use default
                    };
                    Ok(final_node)
                } else {
                    Ok(None)
                }
            }
            NodeSelector::Name { node_type, name } => {
                match language {
                    "rust" => {
                        if let Some(nt) = node_type {
                            // Use the new fallback-aware function
                            return RustParser::find_by_name_with_fallback(
                                tree,
                                source_code,
                                nt,
                                name,
                            );
                        } else {
                            // Try to find by name in any context - this is more complex
                            // For now, try function first, then struct, then enum
                            if let Ok(Some(node)) =
                                RustParser::find_function_by_name(tree, source_code, name)
                            {
                                Ok(Some(node))
                            } else if let Ok(Some(node)) =
                                RustParser::find_struct_by_name(tree, source_code, name)
                            {
                                Ok(Some(node))
                            } else {
                                RustParser::find_enum_by_name(tree, source_code, name)
                            }
                        }
                    }
                    _ => Err(anyhow!(
                        "Unsupported language for name search: {}",
                        language
                    )),
                }
            }
            NodeSelector::Type { node_type } => match language {
                "rust" => {
                    let nodes = RustParser::find_nodes_by_type_with_fallback(tree, node_type);
                    Ok(nodes.into_iter().next())
                }
                _ => Err(anyhow!(
                    "Unsupported language for type search: {}",
                    language
                )),
            },
            NodeSelector::Query { query } => {
                // Generic tree-sitter query execution
                self.execute_query(tree, source_code, language, query)
            }
        }
    }

    pub fn find_node_with_suggestions<'a>(
        &self,
        tree: &'a tree_sitter::Tree,
        source_code: &str,
        language: &str,
    ) -> Result<Option<Node<'a>>> {
        match self.find_node(tree, source_code, language) {
            Ok(Some(node)) => Ok(Some(node)),
            Ok(None) => {
                // Generate helpful suggestions and available options
                let error = match language {
                    "rust" => self.generate_rust_suggestions(tree, source_code),
                    _ => NodeNotFoundError::new(self.clone()),
                };
                Err(anyhow!(error.to_detailed_message()))
            }
            Err(e) => Err(e),
        }
    }

    fn generate_rust_suggestions(
        &self,
        tree: &tree_sitter::Tree,
        source_code: &str,
    ) -> NodeNotFoundError {
        let mut error = NodeNotFoundError::new(self.clone());

        match self {
            NodeSelector::Name { name, node_type } => {
                // Get all available items for suggestions
                let all_functions = RustParser::get_all_function_names(tree, source_code);
                let all_structs = RustParser::get_all_struct_names(tree, source_code);
                let all_enums = RustParser::get_all_enum_names(tree, source_code);
                let all_impls = RustParser::get_all_impl_types(tree, source_code);
                let all_mods = RustParser::get_all_mod_names(tree, source_code);

                let mut available = Vec::new();
                let mut suggestions = Vec::new();

                if node_type.as_deref() == Some("function_item") || node_type.is_none() {
                    available.extend(all_functions.iter().map(|f| format!("function: {f}")));
                    suggestions.extend(Self::fuzzy_match(name, &all_functions));
                }

                if node_type.as_deref() == Some("struct_item") || node_type.is_none() {
                    available.extend(all_structs.iter().map(|s| format!("struct: {s}")));
                    suggestions.extend(Self::fuzzy_match(name, &all_structs));
                }

                if node_type.as_deref() == Some("enum_item") || node_type.is_none() {
                    available.extend(all_enums.iter().map(|e| format!("enum: {e}")));
                    suggestions.extend(Self::fuzzy_match(name, &all_enums));
                }

                if node_type.as_deref() == Some("impl_item") || node_type.is_none() {
                    available.extend(all_impls.iter().map(|i| format!("impl: {i}")));
                    suggestions.extend(Self::fuzzy_match(name, &all_impls));
                }

                if node_type.as_deref() == Some("mod_item") || node_type.is_none() {
                    available.extend(all_mods.iter().map(|m| format!("mod: {m}")));
                    suggestions.extend(Self::fuzzy_match(name, &all_mods));
                }

                error = error
                    .with_available_options(available)
                    .with_suggestions(suggestions);
            }
            NodeSelector::Type { node_type } => {
                // List available node types
                let available_types = Self::get_common_rust_node_types();
                let suggestions = Self::fuzzy_match(node_type, &available_types);
                error = error
                    .with_available_options(available_types)
                    .with_suggestions(suggestions);
            }
            _ => {}
        }

        error
    }

    fn fuzzy_match(input: &str, candidates: &[String]) -> Vec<String> {
        let input_lower = input.to_lowercase();
        let mut matches: Vec<_> = candidates
            .iter()
            .filter_map(|candidate| {
                let candidate_lower = candidate.to_lowercase();
                if candidate_lower.contains(&input_lower) {
                    Some((candidate.clone(), 0)) // Exact substring match gets priority
                } else if Self::levenshtein_distance(&input_lower, &candidate_lower) <= 2 {
                    Some((candidate.clone(), 1)) // Close matches
                } else {
                    None
                }
            })
            .collect();

        matches.sort_by_key(|(_, priority)| *priority);
        matches
            .into_iter()
            .map(|(candidate, _)| candidate)
            .take(3)
            .collect()
    }

    #[allow(clippy::needless_range_loop)]
    fn levenshtein_distance(a: &str, b: &str) -> usize {
        let a_chars: Vec<char> = a.chars().collect();
        let b_chars: Vec<char> = b.chars().collect();
        let mut dp = vec![vec![0; b_chars.len() + 1]; a_chars.len() + 1];

        for i in 0..=a_chars.len() {
            dp[i][0] = i;
        }
        for j in 0..=b_chars.len() {
            dp[0][j] = j;
        }

        for i in 1..=a_chars.len() {
            for j in 1..=b_chars.len() {
                if a_chars[i - 1] == b_chars[j - 1] {
                    dp[i][j] = dp[i - 1][j - 1];
                } else {
                    dp[i][j] = 1 + dp[i - 1][j].min(dp[i][j - 1]).min(dp[i - 1][j - 1]);
                }
            }
        }

        dp[a_chars.len()][b_chars.len()]
    }

    fn get_common_rust_node_types() -> Vec<String> {
        vec![
            "function_item".to_string(),
            "struct_item".to_string(),
            "impl_item".to_string(),
            "enum_item".to_string(),
            "mod_item".to_string(),
            "use_declaration".to_string(),
            "let_declaration".to_string(),
            "expression_statement".to_string(),
            "call_expression".to_string(),
            "macro_invocation".to_string(),
            "if_expression".to_string(),
            "match_expression".to_string(),
            "for_expression".to_string(),
            "while_expression".to_string(),
            "block".to_string(),
        ]
    }

    fn execute_query<'a>(
        &self,
        tree: &'a tree_sitter::Tree,
        source_code: &str,
        language: &str,
        query_text: &str,
    ) -> Result<Option<Node<'a>>> {
        let language_obj = match language {
            "rust" => tree_sitter_rust::LANGUAGE.into(),
            _ => return Err(anyhow!("Unsupported language for queries: {}", language)),
        };

        let query = tree_sitter::Query::new(&language_obj, query_text)?;
        let mut cursor = tree_sitter::QueryCursor::new();

        let mut matches = cursor.matches(&query, tree.root_node(), source_code.as_bytes());
        while let Some(m) = matches.next() {
            if let Some(capture) = m.captures.first() {
                return Ok(Some(capture.node));
            }
        }

        Ok(None)
    }
}

pub fn check_terrible_target(
    selector: &NodeSelector,
    target_node: &tree_sitter::Node<'_>,
    tree: &tree_sitter::Tree,
    source_code: &str,
    language: &str,
) -> Result<Option<String>> {
    use crate::ast_explorer::{ASTExplorer, EditSuitability};

    let node_info = ASTExplorer::analyze_node(target_node, source_code, language);

    if let EditSuitability::Terrible { reason, why_avoid } = node_info.edit_suitability {
        // Only run exploration for position-based selectors (where we have line/column)
        if let NodeSelector::Position { line, column, .. } = selector {
            let exploration =
                ASTExplorer::explore_around(tree, source_code, *line, *column, language)?;

            let mut output = String::new();
            output.push_str(&format!("❌ Edit blocked: {reason}\n"));
            output.push_str(&format!("🚫 {why_avoid}\n\n"));
            output.push_str(&format!(
                "🔍 Auto-exploration at {line}:{column} shows better targets:\n\n",
            ));

            // Find excellent and good targets from ancestors
            let good_targets: Vec<_> = exploration
                .ancestors
                .iter()
                .filter(|node| {
                    matches!(
                        node.edit_suitability,
                        EditSuitability::Excellent { .. } | EditSuitability::Good { .. }
                    )
                })
                .collect();

            if !good_targets.is_empty() {
                output.push_str("✅ **RECOMMENDED TARGETS**:\n");
                for (i, target) in good_targets.iter().take(3).enumerate() {
                    let quality = match target.edit_suitability {
                        EditSuitability::Excellent { .. } => "Excellent",
                        EditSuitability::Good { .. } => "Good",
                        _ => "OK",
                    };

                    output.push_str(&format!(
                        "  {}. {} ({}) - {}\n",
                        i + 1,
                        target.kind,
                        quality,
                        target
                            .semantic_role
                            .as_deref()
                            .unwrap_or("structural element")
                    ));

                    if let Some(selector_opt) = target.selector_options.first() {
                        output.push_str(&format!(
                            "     Selector: {}\n\n",
                            serde_json::to_string(&selector_opt.selector_value).unwrap_or_default()
                        ));
                    }
                }
            }

            output
                .push_str("💡 Use one of the recommended selectors above to target a better node.");
            return Ok(Some(output));
        } else {
            // For non-position selectors, just return a simple error
            return Ok(Some(format!(
                    "❌ Edit blocked: {reason}\n🚫 {why_avoid}\n\n💡 Try using a different selector type or explore_ast to find better targets.",
                )));
        }
    }

    Ok(None) // No terrible target detected
}
