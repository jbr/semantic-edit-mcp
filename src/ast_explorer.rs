use crate::parsers::get_node_text;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use tree_sitter::{Node, Tree};

#[derive(Debug, Serialize, Deserialize)]
pub struct ASTExplorationResult {
    pub focus_node: ASTNodeInfo,
    pub ancestors: Vec<ASTNodeInfo>,
    pub children: Vec<ASTNodeInfo>,
    pub siblings: Vec<ASTNodeInfo>,
    pub context_explanation: String,
    pub edit_recommendations: Vec<EditRecommendation>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ASTNodeInfo {
    pub id: usize,
    pub kind: String,
    pub text_preview: String,
    pub byte_range: (usize, usize),
    pub line_range: (usize, usize),
    pub char_range: (usize, usize),
    pub is_named: bool,
    pub child_count: usize,
    pub selector_options: Vec<NodeSelector>,
    pub edit_suitability: EditSuitability,
    pub semantic_role: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct NodeSelector {
    pub selector_type: String,
    pub selector_value: serde_json::Value,
    pub description: String,
    pub confidence: f32,
}

#[derive(Debug, Serialize, Deserialize)]
pub enum EditSuitability {
    Excellent {
        reason: String,
    },
    Good {
        reason: String,
        considerations: Vec<String>,
    },
    Poor {
        reason: String,
        better_alternatives: Vec<String>,
    },
    Terrible {
        reason: String,
        why_avoid: String,
    },
}

#[derive(Debug, Serialize, Deserialize)]
pub struct EditRecommendation {
    pub target_node_id: usize,
    pub operation: String,
    pub description: String,
    pub selector: serde_json::Value,
    pub confidence: f32,
    pub example_usage: String,
}

pub struct ASTExplorer;

impl ASTExplorer {
    pub fn explore_around(
        tree: &Tree,
        source: &str,
        line: usize,
        column: usize,
        language: &str,
    ) -> Result<ASTExplorationResult> {
        let point = tree_sitter::Point::new(line.saturating_sub(1), column.saturating_sub(1));
        let focus_node = tree
            .root_node()
            .descendant_for_point_range(point, point)
            .ok_or_else(|| anyhow!("No node found at position {}:{}", line, column))?;

        let focus_info = Self::analyze_node(&focus_node, source, language);
        let ancestors = Self::collect_ancestors(&focus_node, source, language);
        let children = Self::collect_children(&focus_node, source, language);
        let siblings = Self::collect_siblings(&focus_node, source, language);

        let context_explanation =
            Self::generate_context_explanation(&focus_info, &ancestors, language);
        let edit_recommendations = Self::generate_edit_recommendations(
            &focus_info,
            &ancestors,
            &children,
            &siblings,
            language,
        );

        Ok(ASTExplorationResult {
            focus_node: focus_info,
            ancestors,
            children,
            siblings,
            context_explanation,
            edit_recommendations,
        })
    }

    pub fn analyze_node(node: &Node, source: &str, language: &str) -> ASTNodeInfo {
        let text = get_node_text(node, source);
        let text_preview = if text.len() > 150 {
            format!("{}...", &text[..147])
        } else {
            text.replace('\n', "\\n")
        };

        let selector_options = Self::generate_selector_options(node, source);
        let edit_suitability = Self::assess_edit_suitability(node, language);
        let semantic_role = Self::identify_semantic_role(node, source, language);

        // Convert byte positions to character positions for easier use
        let rope = ropey::Rope::from_str(source);
        let start_char = rope.byte_to_char(node.start_byte());
        let end_char = rope.byte_to_char(node.end_byte());

        ASTNodeInfo {
            id: node.id(),
            kind: node.kind().to_string(),
            text_preview,
            byte_range: (node.start_byte(), node.end_byte()),
            line_range: (node.start_position().row + 1, node.end_position().row + 1),
            char_range: (start_char, end_char),
            is_named: node.is_named(),
            child_count: node.child_count(),
            selector_options,
            edit_suitability,
            semantic_role,
        }
    }

    fn generate_selector_options(node: &Node, source: &str) -> Vec<NodeSelector> {
        let mut selectors = Vec::new();

        // Type selector (most basic)
        selectors.push(NodeSelector {
            selector_type: "type".to_string(),
            selector_value: serde_json::json!({"type": node.kind()}),
            description: format!("Select by node type: {}", node.kind()),
            confidence: 0.7,
        });

        // Position selector (most precise but fragile)
        selectors.push(NodeSelector {
            selector_type: "position".to_string(),
            selector_value: serde_json::json!({
                "line": node.start_position().row + 1,
                "column": node.start_position().column + 1
            }),
            description: "Select by exact position (fragile to edits)".to_string(),
            confidence: 0.9,
        });

        // Name selector if applicable
        if let Some(name) = Self::extract_node_name(node, source) {
            selectors.push(NodeSelector {
                selector_type: "name".to_string(),
                selector_value: serde_json::json!({"name": name}),
                description: format!("Select by name: {}", name),
                confidence: 0.95,
            });
        }

        // Content-based selector for unique text
        let text = get_node_text(node, source);
        if text.len() < 100 && !text.trim().is_empty() && text.lines().count() <= 3 {
            selectors.push(NodeSelector {
                selector_type: "content".to_string(),
                selector_value: serde_json::json!({"content": text.trim()}),
                description: "Select by exact content match".to_string(),
                confidence: 0.8,
            });
        }

        // Tree-sitter query selector for complex patterns
        if let Some(query) = Self::generate_query_selector(node, source) {
            selectors.push(NodeSelector {
                selector_type: "query".to_string(),
                selector_value: serde_json::json!({"query": query}),
                description: "Select using tree-sitter query".to_string(),
                confidence: 0.85,
            });
        }

        selectors
    }

    fn extract_node_name(node: &Node, source: &str) -> Option<String> {
        // Look for name field or identifier child
        if let Some(name_field) = node.child_by_field_name("name") {
            return Some(get_node_text(&name_field, source).trim().to_string());
        }

        // For function items, struct items, etc., find identifier
        for i in 0..node.child_count() {
            if let Some(child) = node.child(i) {
                if child.kind() == "identifier" || child.kind() == "type_identifier" {
                    return Some(get_node_text(&child, source).trim().to_string());
                }
            }
        }

        None
    }

    fn generate_query_selector(node: &Node, source: &str) -> Option<String> {
        match node.kind() {
            "function_item" => {
                if let Some(name) = Self::extract_node_name(node, source) {
                    Some(format!(
                        "(function_item name: (identifier) @name (#eq? @name \"{}\"))",
                        name
                    ))
                } else {
                    None
                }
            }
            "struct_item" => {
                if let Some(name) = Self::extract_node_name(node, source) {
                    Some(format!(
                        "(struct_item name: (type_identifier) @name (#eq? @name \"{}\"))",
                        name
                    ))
                } else {
                    None
                }
            }
            "atx_heading" => {
                // For markdown headings, try to match by content
                let content = get_node_text(node, source).trim();
                if content.len() < 100 {
                    Some(format!("(atx_heading) @heading"))
                } else {
                    None
                }
            }
            _ => None,
        }
    }

    fn assess_edit_suitability(node: &Node, language: &str) -> EditSuitability {
        match (language, node.kind()) {
            // Rust patterns
            ("rust", "function_item") => EditSuitability::Excellent {
                reason: "Complete function - perfect for replacement or modification".to_string(),
            },
            ("rust", "impl_item") => EditSuitability::Excellent {
                reason: "Complete impl block - ideal for adding methods or replacement".to_string(),
            },
            ("rust", "struct_item") => EditSuitability::Excellent {
                reason: "Complete struct definition".to_string(),
            },
            ("rust", "identifier") => EditSuitability::Poor {
                reason: "Just a name token, not the full construct".to_string(),
                better_alternatives: vec![
                    "Select the parent function_item, struct_item, etc.".to_string()
                ],
            },

            // Markdown patterns
            ("markdown", "document") => EditSuitability::Poor {
                reason: "Entire document - usually too broad".to_string(),
                better_alternatives: vec![
                    "Select specific sections, headings, or lists".to_string()
                ],
            },
            ("markdown", "section") => EditSuitability::Good {
                reason: "Document section with heading and content".to_string(),
                considerations: vec!["May include more content than intended".to_string()],
            },
            ("markdown", "atx_heading") => EditSuitability::Excellent {
                reason: "Complete heading - perfect for heading changes".to_string(),
            },
            ("markdown", "list") => EditSuitability::Excellent {
                reason: "Complete list - ideal for list operations".to_string(),
            },
            ("markdown", "list_item") => EditSuitability::Good {
                reason: "Individual list item".to_string(),
                considerations: vec!["Good for single item changes".to_string()],
            },
            ("markdown", "list_marker_minus" | "list_marker_plus" | "list_marker_star") => {
                EditSuitability::Terrible {
                    reason: "Just the list marker character(s)".to_string(),
                    why_avoid: "This is only the bullet point, not the content".to_string(),
                }
            }
            (
                "markdown",
                "atx_h1_marker" | "atx_h2_marker" | "atx_h3_marker" | "atx_h4_marker"
                | "atx_h5_marker" | "atx_h6_marker",
            ) => EditSuitability::Terrible {
                reason: "Just the heading marker (# ## ###)".to_string(),
                why_avoid: "This is only the hash symbols, not the heading content".to_string(),
            },
            ("markdown", "inline") => EditSuitability::Good {
                reason: "Inline content within paragraph or heading".to_string(),
                considerations: vec!["Good for text content changes".to_string()],
            },
            ("markdown", "fenced_code_block") => EditSuitability::Excellent {
                reason: "Complete code block with language and content".to_string(),
            },

            // JSON patterns
            ("json", "object") => EditSuitability::Excellent {
                reason: "Complete JSON object".to_string(),
            },
            ("json", "array") => EditSuitability::Excellent {
                reason: "Complete JSON array".to_string(),
            },
            ("json", "pair") => EditSuitability::Good {
                reason: "Key-value pair in JSON object".to_string(),
                considerations: vec!["Good for updating specific properties".to_string()],
            },

            // Generic patterns
            (_, kind)
                if kind.contains("_item")
                    || kind.contains("_statement")
                    || kind.contains("_declaration") =>
            {
                EditSuitability::Good {
                    reason: "Appears to be a complete language construct".to_string(),
                    considerations: vec!["Language-specific analysis not available".to_string()],
                }
            }
            (_, kind) if kind.len() == 1 || kind.contains("token") || kind.contains("literal") => {
                EditSuitability::Poor {
                    reason: "Appears to be a small token or literal".to_string(),
                    better_alternatives: vec!["Look for parent nodes with more context".to_string()],
                }
            }
            _ => EditSuitability::Good {
                reason: "Generic node".to_string(),
                considerations: vec![
                    "No specific analysis available for this language/node type".to_string()
                ],
            },
        }
    }

    fn identify_semantic_role(node: &Node, source: &str, language: &str) -> Option<String> {
        match (language, node.kind()) {
            ("rust", "function_item") => Some("Function Definition".to_string()),
            ("rust", "impl_item") => Some("Implementation Block".to_string()),
            ("rust", "struct_item") => Some("Struct Definition".to_string()),
            ("rust", "enum_item") => Some("Enum Definition".to_string()),
            ("rust", "mod_item") => Some("Module Definition".to_string()),
            ("rust", "use_declaration") => Some("Import Statement".to_string()),

            ("markdown", "atx_heading") => {
                let level = if node.child(0).map(|c| c.kind()).unwrap_or("") == "atx_h1_marker" {
                    "1"
                } else if node.child(0).map(|c| c.kind()).unwrap_or("") == "atx_h2_marker" {
                    "2"
                } else if node.child(0).map(|c| c.kind()).unwrap_or("") == "atx_h3_marker" {
                    "3"
                } else {
                    "N"
                };
                Some(format!("Heading Level {}", level))
            }
            ("markdown", "list") => Some("List".to_string()),
            ("markdown", "list_item") => Some("List Item".to_string()),
            ("markdown", "fenced_code_block") => {
                // Try to get language from info_string
                let lang = node
                    .children(&mut node.walk())
                    .find(|c| c.kind() == "info_string")
                    .and_then(|info| {
                        info.children(&mut info.walk())
                            .find(|c| c.kind() == "language")
                    })
                    .map(|lang_node| get_node_text(&lang_node, source).trim().to_string())
                    .unwrap_or_else(|| "unknown".to_string());
                Some(format!("Code Block ({})", lang))
            }
            ("markdown", "block_quote") => Some("Block Quote".to_string()),
            ("markdown", "section") => Some("Document Section".to_string()),

            ("json", "object") => Some("JSON Object".to_string()),
            ("json", "array") => Some("JSON Array".to_string()),
            ("json", "pair") => Some("Key-Value Pair".to_string()),

            _ => None,
        }
    }

    fn collect_ancestors(node: &Node, source: &str, language: &str) -> Vec<ASTNodeInfo> {
        let mut ancestors = Vec::new();
        let mut current = node.parent();

        while let Some(parent) = current {
            ancestors.push(Self::analyze_node(&parent, source, language));
            current = parent.parent();

            // Prevent infinite recursion and limit depth
            if ancestors.len() > 20 {
                break;
            }
        }

        ancestors
    }

    fn collect_children(node: &Node, source: &str, language: &str) -> Vec<ASTNodeInfo> {
        (0..node.child_count())
            .filter_map(|i| node.child(i))
            .map(|child| Self::analyze_node(&child, source, language))
            .collect()
    }

    fn collect_siblings(node: &Node, source: &str, language: &str) -> Vec<ASTNodeInfo> {
        let Some(parent) = node.parent() else {
            return Vec::new();
        };

        (0..parent.child_count())
            .filter_map(|i| parent.child(i))
            .filter(|sibling| sibling.id() != node.id())
            .map(|sibling| Self::analyze_node(&sibling, source, language))
            .collect()
    }

    fn generate_context_explanation(
        focus: &ASTNodeInfo,
        ancestors: &[ASTNodeInfo],
        language: &str,
    ) -> String {
        let mut explanation = format!(
            "🎯 Focus: {} ({})",
            focus.kind,
            focus.semantic_role.as_deref().unwrap_or("structural")
        );

        if !focus.text_preview.is_empty() {
            explanation.push_str(&format!("\n   Content: \"{}\"", focus.text_preview));
        }

        explanation.push_str(&format!(
            "\n   Position: lines {}-{}, chars {}-{}",
            focus.line_range.0, focus.line_range.1, focus.char_range.0, focus.char_range.1
        ));

        if !ancestors.is_empty() {
            explanation.push_str("\n\n📍 Context hierarchy (inner → outer):");
            for (i, ancestor) in ancestors.iter().enumerate() {
                let indent = "  ".repeat(i + 1);
                let role = ancestor.semantic_role.as_deref().unwrap_or("structural");
                explanation.push_str(&format!("\n{}{} ({})", indent, ancestor.kind, role));

                if !ancestor.text_preview.is_empty() && ancestor.text_preview.len() < 50 {
                    explanation.push_str(&format!(" - \"{}\"", ancestor.text_preview));
                }
            }
        }

        // Add edit suitability assessment
        match &focus.edit_suitability {
            EditSuitability::Excellent { reason } => {
                explanation.push_str(&format!("\n\n✅ Excellent edit target: {}", reason));
            }
            EditSuitability::Good {
                reason,
                considerations,
            } => {
                explanation.push_str(&format!("\n\n✓ Good edit target: {}", reason));
                if !considerations.is_empty() {
                    explanation
                        .push_str(&format!("\n   💡 Consider: {}", considerations.join(", ")));
                }
            }
            EditSuitability::Poor {
                reason,
                better_alternatives,
            } => {
                explanation.push_str(&format!("\n\n⚠️ Poor edit target: {}", reason));
                if !better_alternatives.is_empty() {
                    explanation.push_str(&format!(
                        "\n   💡 Try instead: {}",
                        better_alternatives.join(", ")
                    ));
                }
            }
            EditSuitability::Terrible { reason, why_avoid } => {
                explanation.push_str(&format!("\n\n❌ Avoid editing this: {}", reason));
                explanation.push_str(&format!("\n   🚫 {}", why_avoid));
            }
        }

        explanation
    }

    fn generate_edit_recommendations(
        focus: &ASTNodeInfo,
        ancestors: &[ASTNodeInfo],
        _children: &[ASTNodeInfo],
        _siblings: &[ASTNodeInfo],
        language: &str,
    ) -> Vec<EditRecommendation> {
        let mut recommendations = Vec::new();

        // If current node is terrible/poor, recommend better ancestors
        if matches!(
            focus.edit_suitability,
            EditSuitability::Terrible { .. } | EditSuitability::Poor { .. }
        ) {
            for ancestor in ancestors.iter() {
                if matches!(
                    ancestor.edit_suitability,
                    EditSuitability::Excellent { .. } | EditSuitability::Good { .. }
                ) {
                    recommendations.push(EditRecommendation {
                        target_node_id: ancestor.id,
                        operation: "replace_node".to_string(),
                        description: format!("Replace {} instead of {}", ancestor.kind, focus.kind),
                        selector: ancestor
                            .selector_options
                            .first()
                            .map(|s| s.selector_value.clone())
                            .unwrap_or_else(|| serde_json::json!({"type": ancestor.kind})),
                        confidence: 0.9,
                        example_usage: format!(
                            "replace_node(file, {}, new_content)",
                            serde_json::to_string(
                                &ancestor
                                    .selector_options
                                    .first()
                                    .unwrap_or(&NodeSelector {
                                        selector_type: "type".to_string(),
                                        selector_value: serde_json::json!({"type": ancestor.kind}),
                                        description: "fallback".to_string(),
                                        confidence: 0.5,
                                    })
                                    .selector_value
                            )
                            .unwrap()
                        ),
                    });
                    break; // Only suggest the first good ancestor
                }
            }
        }

        // Language-specific recommendations
        match language {
            "markdown" => {
                Self::generate_markdown_recommendations(focus, ancestors, &mut recommendations)
            }
            "rust" => Self::generate_rust_recommendations(focus, ancestors, &mut recommendations),
            "json" => Self::generate_json_recommendations(focus, ancestors, &mut recommendations),
            _ => {}
        }

        recommendations
    }

    fn generate_markdown_recommendations(
        focus: &ASTNodeInfo,
        ancestors: &[ASTNodeInfo],
        recommendations: &mut Vec<EditRecommendation>,
    ) {
        match focus.kind.as_str() {
            "atx_heading" => {
                recommendations.push(EditRecommendation {
                    target_node_id: focus.id,
                    operation: "replace_node".to_string(),
                    description: "Replace heading text".to_string(),
                    selector: serde_json::json!({"type": "atx_heading"}),
                    confidence: 0.95,
                    example_usage:
                        "replace_node(file, {\"type\": \"atx_heading\"}, \"# New Heading\")"
                            .to_string(),
                });

                recommendations.push(EditRecommendation {
                    target_node_id: focus.id,
                    operation: "insert_after_node".to_string(),
                    description: "Add content after this heading".to_string(),
                    selector: serde_json::json!({"type": "atx_heading"}),
                    confidence: 0.9,
                    example_usage: "insert_after_node(file, {\"type\": \"atx_heading\"}, \"\\n\\nNew content\")".to_string(),
                });
            }
            "list" => {
                recommendations.push(EditRecommendation {
                    target_node_id: focus.id,
                    operation: "insert_after_node".to_string(),
                    description: "Add new item to this list".to_string(),
                    selector: serde_json::json!({"type": "list"}),
                    confidence: 0.9,
                    example_usage:
                        "insert_after_node(file, {\"type\": \"list\"}, \"\\n- New item\")"
                            .to_string(),
                });
            }
            "list_item" => {
                recommendations.push(EditRecommendation {
                    target_node_id: focus.id,
                    operation: "replace_node".to_string(),
                    description: "Replace this list item".to_string(),
                    selector: serde_json::json!({"type": "list_item"}),
                    confidence: 0.85,
                    example_usage:
                        "replace_node(file, {\"type\": \"list_item\"}, \"- Updated item\")"
                            .to_string(),
                });
            }
            "section" => {
                recommendations.push(EditRecommendation {
                    target_node_id: focus.id,
                    operation: "insert_after_node".to_string(),
                    description: "Add new section after this one".to_string(),
                    selector: serde_json::json!({"type": "section"}),
                    confidence: 0.8,
                    example_usage: "insert_after_node(file, {\"type\": \"section\"}, \"\\n\\n## New Section\\n\\nContent\")".to_string(),
                });
            }
            _ => {}
        }
    }

    fn generate_rust_recommendations(
        focus: &ASTNodeInfo,
        _ancestors: &[ASTNodeInfo],
        recommendations: &mut Vec<EditRecommendation>,
    ) {
        match focus.kind.as_str() {
            "function_item" => {
                recommendations.push(EditRecommendation {
                    target_node_id: focus.id,
                    operation: "replace_node".to_string(),
                    description: "Replace entire function".to_string(),
                    selector: serde_json::json!({"type": "function_item"}),
                    confidence: 0.95,
                    example_usage: "replace_node(file, {\"type\": \"function_item\"}, \"fn new_function() { ... }\")".to_string(),
                });

                if let Some(name_selector) = focus
                    .selector_options
                    .iter()
                    .find(|s| s.selector_type == "name")
                {
                    recommendations.push(EditRecommendation {
                        target_node_id: focus.id,
                        operation: "replace_node".to_string(),
                        description: "Replace function by name (safer)".to_string(),
                        selector: name_selector.selector_value.clone(),
                        confidence: 0.98,
                        example_usage: format!(
                            "replace_node(file, {}, \"fn new_function() {{ ... }}\")",
                            serde_json::to_string(&name_selector.selector_value).unwrap()
                        ),
                    });
                }
            }
            "impl_item" => {
                recommendations.push(EditRecommendation {
                    target_node_id: focus.id,
                    operation: "insert_after_impl".to_string(),
                    description: "Add method to impl block".to_string(),
                    selector: serde_json::json!({"type": "impl_item"}),
                    confidence: 0.9,
                    example_usage:
                        "insert_after_impl(file, impl_type, \"fn new_method(&self) { ... }\")"
                            .to_string(),
                });
            }
            _ => {}
        }
    }

    fn generate_json_recommendations(
        focus: &ASTNodeInfo,
        _ancestors: &[ASTNodeInfo],
        recommendations: &mut Vec<EditRecommendation>,
    ) {
        match focus.kind.as_str() {
            "object" => {
                recommendations.push(EditRecommendation {
                    target_node_id: focus.id,
                    operation: "insert_after_node".to_string(),
                    description: "Add property to JSON object".to_string(),
                    selector: serde_json::json!({"type": "object"}),
                    confidence: 0.85,
                    example_usage: "insert_after_node(file, {\"type\": \"object\"}, \", \\\"new_key\\\": \\\"value\\\"\")".to_string(),
                });
            }
            "array" => {
                recommendations.push(EditRecommendation {
                    target_node_id: focus.id,
                    operation: "insert_after_node".to_string(),
                    description: "Add item to JSON array".to_string(),
                    selector: serde_json::json!({"type": "array"}),
                    confidence: 0.85,
                    example_usage:
                        "insert_after_node(file, {\"type\": \"array\"}, \", \\\"new_item\\\"\")"
                            .to_string(),
                });
            }
            _ => {}
        }
    }
}
