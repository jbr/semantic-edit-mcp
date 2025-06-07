use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use tree_sitter::{Node, StreamingIterator};

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
pub struct EditResult {
    pub success: bool,
    pub message: String,
    pub new_content: Option<String>,
    pub affected_range: Option<(usize, usize)>,
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

    pub fn apply(&self, source_code: &str, language: &str) -> Result<EditResult> {
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
            _ => Err(anyhow!("Unsupported language for editing: {}", language)),
        }
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
                            return parser.find_by_name(tree, source_code, nt, name);
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
                            match nt.as_str() {
                                "function_item" => {
                                    crate::parsers::rust::RustParser::find_function_by_name(
                                        tree,
                                        source_code,
                                        name,
                                    )
                                }
                                "struct_item" => {
                                    crate::parsers::rust::RustParser::find_struct_by_name(
                                        tree,
                                        source_code,
                                        name,
                                    )
                                }
                                "enum_item" => crate::parsers::rust::RustParser::find_enum_by_name(
                                    tree,
                                    source_code,
                                    name,
                                ),
                                _ => Err(anyhow!("Unsupported node type for name search: {}", nt)),
                            }
                        } else {
                            // Try to find by name in any context - this is more complex
                            // For now, try function first, then struct, then enum
                            if let Ok(Some(node)) =
                                crate::parsers::rust::RustParser::find_function_by_name(
                                    tree,
                                    source_code,
                                    name,
                                )
                            {
                                Ok(Some(node))
                            } else if let Ok(Some(node)) =
                                crate::parsers::rust::RustParser::find_struct_by_name(
                                    tree,
                                    source_code,
                                    name,
                                )
                            {
                                Ok(Some(node))
                            } else {
                                crate::parsers::rust::RustParser::find_enum_by_name(
                                    tree,
                                    source_code,
                                    name,
                                )
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
                    let nodes =
                        crate::parsers::rust::RustParser::find_nodes_by_type(tree, node_type);
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
                let all_functions =
                    crate::parsers::rust::RustParser::get_all_function_names(tree, source_code);
                let all_structs =
                    crate::parsers::rust::RustParser::get_all_struct_names(tree, source_code);
                let all_enums =
                    crate::parsers::rust::RustParser::get_all_enum_names(tree, source_code);
                let all_impls =
                    crate::parsers::rust::RustParser::get_all_impl_types(tree, source_code);
                let all_mods =
                    crate::parsers::rust::RustParser::get_all_mod_names(tree, source_code);

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
