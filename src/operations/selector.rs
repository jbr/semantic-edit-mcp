use std::collections::HashSet;

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tree_sitter::{Node, Tree};

/// Text-anchored node selector using content as anchor points and AST structure for navigation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSelector {
    /// Exact text to find in the source code as an anchor point
    pub anchor_text: String,
    /// AST node type to walk up to from the anchor point (optional - when omitted, returns exploration data)
    pub ancestor_node_type: Option<String>,
}

impl NodeSelector {
    pub fn new_from_value(args: &Value) -> Result<Self> {
        let selector_obj = args
            .get("selector")
            .ok_or_else(|| anyhow!("selector is required"))?
            .as_object()
            .ok_or_else(|| anyhow!("selector must be an object"))?;

        if let Some(anchor_text) = selector_obj.get("anchor_text").and_then(|v| v.as_str()) {
            let ancestor_node_type = selector_obj
                .get("ancestor_node_type")
                .and_then(|v| v.as_str())
                .map(|s| s.to_string());

            return Ok(NodeSelector {
                anchor_text: anchor_text.to_string(),
                ancestor_node_type,
            });
        }

        Err(anyhow!(
            "Invalid selector: must specify:\n\
             • Text-anchored: {{\"anchor_text\": \"exact text\"}}\n\
             • With targeting: {{\"anchor_text\": \"exact text\", \"ancestor_node_type\": \"node_type\"}}\n\
             \n\
             💡 Omit ancestor_node_type to explore available options around your anchor text."
        ))
    }

    /// Find a node using text-anchored selection
    pub fn find_node<'a>(&self, tree: &'a Tree, source_code: &str) -> Result<Node<'a>, String> {
        // Text Search: Find all exact matches of anchor_text
        let anchor_positions = find_text_positions(&self.anchor_text, source_code);

        if anchor_positions.is_empty() {
            return Err(format!(
                "anchor_text {:?} not found in file",
                self.anchor_text
            ));
        }

        // Check if this is exploration mode (no ancestor_node_type specified)
        if self.ancestor_node_type.is_none() {
            return Err(self.explore_around_anchors(tree, source_code, &anchor_positions));
        }

        // Specific targeting mode - find exact ancestor type
        let ancestor_node_type = self.ancestor_node_type.as_ref().unwrap();

        // Convert positions to nodes and walk up to find ancestors
        let mut valid_targets = Vec::new();
        let mut anchor_info = Vec::new();

        for &byte_pos in &anchor_positions {
            // Convert byte position to tree-sitter node
            if let Some(anchor_node) = find_node_at_byte_position(tree, byte_pos) {
                // Walk up to find the desired ancestor type
                if let Some(target_node) = find_ancestor_of_type(&anchor_node, ancestor_node_type) {
                    valid_targets.push(target_node);

                    // Get context for error reporting
                    let (line, _column) = byte_position_to_line_column(source_code, byte_pos);
                    let ancestor_chain = get_ancestor_chain(&anchor_node);
                    anchor_info.push(AnchorInfo {
                        line,
                        // column,
                        byte_pos,
                        ancestor_chain,
                        found_target: true,
                    });
                } else {
                    // Record info about failed ancestor search
                    let (line, _column) = byte_position_to_line_column(source_code, byte_pos);
                    let ancestor_chain = get_ancestor_chain(&anchor_node);
                    anchor_info.push(AnchorInfo {
                        line,
                        // column,
                        byte_pos,
                        ancestor_chain,
                        found_target: false,
                    });
                }
            }
        }

        // Uniqueness Validation
        match valid_targets.len() {
            0 => {
                // No valid targets found - provide detailed error
                Err(format_no_ancestor_error(
                    &self.anchor_text,
                    ancestor_node_type,
                    &anchor_info,
                    source_code,
                ))
            }
            1 => {
                // Perfect! Exactly one valid target
                Ok(valid_targets[0])
            }
            _ => {
                // Multiple valid targets - ambiguous selector
                Err(format_ambiguous_error(
                    &self.anchor_text,
                    ancestor_node_type,
                    &anchor_info,
                    source_code,
                ))
            }
        }
    }

    /// Find node with enhanced error messages and suggestions
    pub fn find_node_with_suggestions<'a>(
        &self,
        tree: &'a Tree,
        source_code: &str,
    ) -> Result<Node<'a>, String> {
        self.find_node(tree, source_code)
    }

    /// Exploration mode: return information about available targeting options
    fn explore_around_anchors(
        &self,
        tree: &Tree,
        source_code: &str,
        anchor_positions: &[usize],
    ) -> String {
        let mut exploration_report = String::new();
        exploration_report.push_str(&format!(
            "🔍 **Exploration Mode**: Found anchor_text {:?} at {} location(s)\n\n",
            self.anchor_text,
            anchor_positions.len()
        ));

        exploration_report.push_str("💡 **Discovery**: Omit ancestor_node_type to explore what's available around your anchor text.\n\n");

        // Collect information about each anchor location
        for (i, &byte_pos) in anchor_positions.iter().enumerate() {
            let (line, column) = byte_position_to_line_column(source_code, byte_pos);
            exploration_report.push_str(&format!("**{}. Location {}:{}**\n", i + 1, line, column));

            if let Some(anchor_node) = find_node_at_byte_position(tree, byte_pos) {
                // Get context around this position
                if let Some(context) = get_context_around_position(source_code, byte_pos, 50) {
                    exploration_report.push_str(&format!("   Context: \"{context}\"\n"));
                }

                // Show available ancestor types
                let ancestor_chain = get_ancestor_chain(&anchor_node);
                if !ancestor_chain.is_empty() {
                    exploration_report.push_str("   Available ancestor_node_type options:\n");
                    for (j, ancestor_type) in ancestor_chain.iter().enumerate() {
                        exploration_report.push_str(&format!("   • \"{ancestor_type}\"\n"));

                        // Show selector example for the first few options
                        if j < 3 {
                            exploration_report.push_str(&format!(
                                "     {{\"anchor_text\": \"{}\", \"ancestor_node_type\": \"{}\"}}\n",
                                self.anchor_text, ancestor_type
                            ));
                        }
                    }
                }

                // Show current focus node and its position in hierarchy
                exploration_report.push_str(&format!("   Focus node: {}\n", anchor_node.kind()));

                // Show structural context: what contains what
                if let Some(parent) = anchor_node.parent() {
                    exploration_report.push_str(&format!(
                        "   Parent: {} → {}\n",
                        parent.kind(),
                        anchor_node.kind()
                    ));
                }
            }
            exploration_report.push('\n');
        }

        exploration_report.push_str("**Next Steps**:\n");
        exploration_report.push_str("1. Use the read_documentation tool to learn the full set of node types for this language if you have not yet done so\n");
        exploration_report.push_str("2. Pick an ancestor_node_type from the options above\n");
        exploration_report.push_str("3. Pick an ancestor_node_type from the options above\n");
        exploration_report.push_str("4. Use preview_only: true to verify your selection\n");
        exploration_report.push_str("5. Run your edit operation\n");

        // Return exploration results as an error (this prevents actual editing)
        exploration_report
    }
}

/// Information about an anchor point found in the text
#[derive(Debug)]
struct AnchorInfo {
    line: usize,
    //column: usize,
    byte_pos: usize,
    ancestor_chain: Vec<String>,
    found_target: bool,
}

/// Find all byte positions where the anchor text appears
fn find_text_positions(anchor_text: &str, source_code: &str) -> Vec<usize> {
    let mut positions = Vec::new();
    let mut start = 0;

    while let Some(pos) = source_code[start..].find(anchor_text) {
        positions.push(start + pos);
        start += pos + 1; // Move past this match to find overlapping matches
    }

    positions
}

/// Find the tree-sitter node at a specific byte position
fn find_node_at_byte_position<'a>(tree: &'a Tree, byte_pos: usize) -> Option<Node<'a>> {
    let root = tree.root_node();

    // Find the smallest node that contains this byte position
    let mut current = root;

    loop {
        let mut found_child = false;
        for i in 0..current.child_count() {
            if let Some(child) = current.child(i) {
                if child.start_byte() <= byte_pos && byte_pos < child.end_byte() {
                    current = child;
                    found_child = true;
                    break;
                }
            }
        }

        if !found_child {
            break;
        }
    }

    Some(current)
}

/// Walk up the AST to find an ancestor of the specified type
fn find_ancestor_of_type<'a>(node: &Node<'a>, target_type: &str) -> Option<Node<'a>> {
    let mut current = *node;

    while let Some(parent) = current.parent() {
        if parent.kind() == target_type {
            return Some(parent);
        }
        current = parent;
    }

    None
}

/// Get the chain of ancestor node types for error reporting
fn get_ancestor_chain(node: &Node) -> Vec<String> {
    let mut chain = Vec::new();
    let mut current = *node;

    while let Some(parent) = current.parent() {
        chain.push(parent.kind().to_string());
        current = parent;
    }

    chain
}

/// Convert byte position to (line, column) for error reporting
fn byte_position_to_line_column(source_code: &str, byte_pos: usize) -> (usize, usize) {
    let mut line = 1;
    let mut column = 1;

    for (i, ch) in source_code.char_indices() {
        if i >= byte_pos {
            break;
        }

        if ch == '\n' {
            line += 1;
            column = 1;
        } else {
            column += 1;
        }
    }

    (line, column)
}

/// Format error message when no ancestor is found
fn format_no_ancestor_error(
    anchor_text: &str,
    ancestor_node_type: &str,
    anchor_info: &[AnchorInfo],
    source_code: &str,
) -> String {
    let total_anchors = anchor_info.len();
    let valid_targets = anchor_info.iter().filter(|info| info.found_target).count();

    if valid_targets == 0 && total_anchors > 0 {
        // Found anchor text but no valid ancestors
        let mut message = format!(
            "Error: anchor_text {anchor_text:?} appears {total_anchors} time(s) but no instances have ancestor {ancestor_node_type:?}
Suggestion: use the read_documentation tool if you have not yet done so for this language
"
        );

        // Show available ancestor types
        let all_ancestors: HashSet<String> = anchor_info
            .iter()
            .flat_map(|info| info.ancestor_chain.iter().cloned())
            .collect();

        if !all_ancestors.is_empty() {
            message.push_str("Available ancestor types: ");
            let mut ancestors: Vec<_> = all_ancestors.iter().cloned().collect();
            ancestors.sort();
            message.push_str(&ancestors.join(", "));
            message.push('\n');
        }

        // Show context for each anchor position
        for (_i, info) in anchor_info.iter().enumerate() {
            message.push_str(&format!("\n{}. Line {}: ", _i + 1, info.line));
            if let Some(context) = get_context_around_position(source_code, info.byte_pos, 40) {
                message.push_str(&context);
            }

            if !info.ancestor_chain.is_empty() {
                message.push_str(&format!(
                    "\n   Available ancestors: {}",
                    info.ancestor_chain.join(", ")
                ));
            }
        }

        if let Some(suggestion) = suggest_ancestor_type(&all_ancestors, ancestor_node_type) {
            message.push_str(&format!(
                "\nSuggestion: Try ancestor_node_type {suggestion:?} instead"
            ));
        }

        message
    } else {
        format!("anchor_text {anchor_text:?} not found in file")
    }
}

/// Format error message when multiple valid targets are found
fn format_ambiguous_error(
    anchor_text: &str,
    ancestor_node_type: &str,
    anchor_info: &[AnchorInfo],
    source_code: &str,
) -> String {
    let valid_count = anchor_info.iter().filter(|info| info.found_target).count();

    let mut message = format!(
        "Error: anchor_text {anchor_text:?} with ancestor_node_type {ancestor_node_type:?} matches {valid_count} nodes\nLocations:\n"
    );

    for info in anchor_info.iter().filter(|info| info.found_target) {
        message.push_str(&format!("  - Line {}: ", info.line));
        if let Some(context) = get_context_around_position(source_code, info.byte_pos, 30) {
            message.push_str(&context);
        }
        message.push('\n');
    }

    message.push_str(
        "Suggestions: 
-> Use more specific anchor text to distinguish between matches
-> use the read_documentation tool if you have not yet done so",
    );
    message
}

/// Get text context around a byte position
fn get_context_around_position(
    source_code: &str,
    byte_pos: usize,
    context_chars: usize,
) -> Option<String> {
    let start = byte_pos.saturating_sub(context_chars);
    let end = (byte_pos + context_chars).min(source_code.len());

    if start < source_code.len() && end <= source_code.len() {
        let context = &source_code[start..end];
        Some(context.replace('\n', "\\n").replace('\t', "\\t"))
    } else {
        None
    }
}

/// Suggest a similar ancestor type based on fuzzy matching
fn suggest_ancestor_type(available: &HashSet<String>, target: &str) -> Option<String> {
    let target_lower = target.to_lowercase();

    // Look for exact substring matches first
    for ancestor in available {
        if ancestor.to_lowercase().contains(&target_lower) {
            return Some(ancestor.clone());
        }
    }

    // Look for similar length matches
    for ancestor in available {
        if levenshtein_distance(&target_lower, &ancestor.to_lowercase()) <= 2 {
            return Some(ancestor.clone());
        }
    }

    None
}

/// Simple Levenshtein distance calculation
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
