use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use tree_sitter::{Node, Tree};

/// Text-anchored node selector using content as anchor points and AST structure for navigation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSelector {
    /// Exact text to find in the source code as an anchor point
    pub anchor_text: String,
    /// AST node type to walk up to from the anchor point
    pub ancestor_node_type: String,
}

impl NodeSelector {
    /// Find a node using text-anchored selection
    pub fn find_node<'a>(
        &self,
        tree: &'a Tree,
        source_code: &str,
        _language: &str,
    ) -> Result<Option<Node<'a>>> {
        // 1. Text Search: Find all exact matches of anchor_text
        let anchor_positions = find_text_positions(&self.anchor_text, source_code);

        if anchor_positions.is_empty() {
            return Err(anyhow!(
                "anchor_text {:?} not found in file",
                self.anchor_text
            ));
        }

        // 2. Convert positions to nodes and walk up to find ancestors
        let mut valid_targets = Vec::new();
        let mut anchor_info = Vec::new();

        for &byte_pos in &anchor_positions {
            // Convert byte position to tree-sitter node
            if let Some(anchor_node) = find_node_at_byte_position(tree, byte_pos) {
                // Walk up to find the desired ancestor type
                if let Some(target_node) = find_ancestor_of_type(&anchor_node, &self.ancestor_node_type) {
                    valid_targets.push(target_node);
                    
                    // Get context for error reporting
                    let (line, column) = byte_position_to_line_column(source_code, byte_pos);
                    let ancestor_chain = get_ancestor_chain(&anchor_node);
                    anchor_info.push(AnchorInfo {
                        line,
                        column,
                        byte_pos,
                        ancestor_chain,
                        found_target: true,
                    });
                } else {
                    // Record info about failed ancestor search
                    let (line, column) = byte_position_to_line_column(source_code, byte_pos);
                    let ancestor_chain = get_ancestor_chain(&anchor_node);
                    anchor_info.push(AnchorInfo {
                        line,
                        column,
                        byte_pos,
                        ancestor_chain,
                        found_target: false,
                    });
                }
            }
        }

        // 3. Uniqueness Validation
        match valid_targets.len() {
            0 => {
                // No valid targets found - provide detailed error
                Err(anyhow!(format_no_ancestor_error(
                    &self.anchor_text,
                    &self.ancestor_node_type,
                    &anchor_info,
                    source_code
                )))
            }
            1 => {
                // Perfect! Exactly one valid target
                Ok(Some(valid_targets[0]))
            }
            _ => {
                // Multiple valid targets - ambiguous selector
                Err(anyhow!(format_ambiguous_error(
                    &self.anchor_text,
                    &self.ancestor_node_type,
                    &anchor_info,
                    source_code
                )))
            }
        }
    }

    /// Find node with enhanced error messages and suggestions
    pub fn find_node_with_suggestions<'a>(
        &self,
        tree: &'a Tree,
        source_code: &str,
        language: &str,
    ) -> Result<Option<Node<'a>>> {
        self.find_node(tree, source_code, language)
    }
}

/// Information about an anchor point found in the text
#[derive(Debug)]
struct AnchorInfo {
    line: usize,
    column: usize,
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
            "Error: anchor_text {:?} appears {} time(s) but no instances have ancestor {:?}\n",
            anchor_text, total_anchors, ancestor_node_type
        );
        
        // Show available ancestor types
        let all_ancestors: std::collections::HashSet<String> = anchor_info
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
                message.push_str(&format!("\n   Available ancestors: {}", info.ancestor_chain.join(", ")));
            }
        }
        
        if let Some(suggestion) = suggest_ancestor_type(&all_ancestors, ancestor_node_type) {
            message.push_str(&format!("\nSuggestion: Try ancestor_node_type {:?} instead", suggestion));
        }
        
        message
    } else {
        format!("anchor_text {:?} not found in file", anchor_text)
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
        "Error: anchor_text {:?} with ancestor_node_type {:?} matches {} nodes\nLocations:\n",
        anchor_text, ancestor_node_type, valid_count
    );
    
    for (_i, info) in anchor_info.iter().filter(|info| info.found_target).enumerate() {
        message.push_str(&format!("  - Line {}: ", info.line));
        if let Some(context) = get_context_around_position(source_code, info.byte_pos, 30) {
            message.push_str(&context);
        }
        message.push('\n');
    }
    
    message.push_str("Suggestion: Use more specific anchor text to distinguish between matches");
    message
}

/// Get text context around a byte position
fn get_context_around_position(source_code: &str, byte_pos: usize, context_chars: usize) -> Option<String> {
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
fn suggest_ancestor_type(available: &std::collections::HashSet<String>, target: &str) -> Option<String> {
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
