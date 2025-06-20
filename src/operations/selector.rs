use std::{collections::HashSet, ops::Range};

use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tree_sitter::{Node, Tree};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Position {
    Before,
    After,
    Replace,
    Around,
}

/// Text-anchored node selector using content as anchor points and AST structure for navigation
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NodeSelector {
    /// Where to place the content
    pub position: Option<Position>,

    /// Exact text to find in the source code as an anchor point
    pub anchor_text: String,

    /// AST node type to walk up to from the anchor point (optional - when omitted, returns exploration data)
    pub ancestor_node_type: Option<String>,
}

impl NodeSelector {
    pub fn new_from_value(args: &Value) -> Result<Self> {
        let selector_obj = args
            .get("selector")
            .ok_or_else(|| anyhow!("selector is required"))?;

        serde_json::from_value(selector_obj.clone()).map_err(|e|
            anyhow!(
                "{e}\n\nselector must specify:\n\
             • Explore mode: {{\"anchor_text\": \"exact text\" }}\n\
             • With targeting: {{\"anchor_text\": \"exact text\", \"ancestor_node_type\": \"node type\", \"position\": POSITION}}\n\
             where POSITION is one of \"before\", \"after\", \"replace\", or \"around\"\n\
             \n\
             💡 Omit ancestor_node_type to explore available options around your anchor text."
            )
        )
    }

    /// Find a node using text-anchored selection
    pub fn find_node<'a>(&self, tree: &'a Tree, source_code: &str) -> Result<Node<'a>, String> {
        // Text Search: Find all exact matches of anchor_text
        let anchor_positions = source_code
            .match_indices(&self.anchor_text)
            .map(|(index, _)| index)
            .collect::<Vec<_>>();

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
            if let Some(anchor_node) =
                find_node_at_byte_position(tree, byte_pos, self.anchor_text.len())
            {
                // Walk up to find the desired ancestor type
                if let Some(target_node) = find_ancestor_of_type(&anchor_node, ancestor_node_type) {
                    valid_targets.push(target_node);

                    // Get context for error reporting
                    let ancestor_chain = get_ancestor_chain(&anchor_node, source_code);
                    anchor_info.push(AnchorInfo {
                        target_node: Some(target_node),
                        anchor_node,
                        ancestor_chain,
                        found_target: true,
                    });
                } else {
                    // Record info about failed ancestor search
                    let ancestor_chain = get_ancestor_chain(&anchor_node, source_code);
                    anchor_info.push(AnchorInfo {
                        target_node: None,
                        anchor_node,
                        ancestor_chain,
                        found_target: false,
                    });
                }
            }
        }

        valid_targets.dedup();
        anchor_info
            .dedup_by_key(|anchor_info| anchor_info.target_node.unwrap_or(anchor_info.anchor_node));

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

        // Collect information about each anchor location
        for (i, &byte_pos) in anchor_positions.iter().enumerate() {
            let (line, column) = byte_position_to_line_column(source_code, byte_pos);
            exploration_report.push_str(&format!("**{}. Location {}:{}**\n", i + 1, line, column));

            if let Some(anchor_node) =
                find_node_at_byte_position(tree, byte_pos, self.anchor_text.len())
            {
                // Get context around this position
                if let Some(context) =
                    get_context_around_position(&anchor_node, source_code, &self.anchor_text, 50)
                {
                    exploration_report.push_str(&format!("   Context: `{context}`\n"));
                }

                // Show current focus node and its position in hierarchy
                exploration_report.push_str(&format!("   Focus node: `{}`\n", anchor_node.kind()));

                // Show available ancestor types
                let ancestor_chain = get_ancestor_chain(&anchor_node, source_code);
                if !ancestor_chain.is_empty() {
                    exploration_report.push_str("   Available ancestor_node_type options:\n");
                    for (j, ancestor) in ancestor_chain.iter().enumerate() {
                        exploration_report.push_str(&ancestor.to_string());

                        // Show selector example for the first few options
                        if j < 3 {
                            exploration_report.push_str(&format!(
                                "      -> EXAMPLE SELECTOR: {{\"anchor_text\": \"{}\", \"ancestor_node_type\": \"{}\"}}\n",
                                self.anchor_text, ancestor.kind
                            ));
                        }
                    }
                }
            }
            exploration_report.push('\n');
        }

        exploration_report.push_str("**Next Steps**:\n");
        exploration_report.push_str("1. Use the open_file tool to see the ast structure of this document if you have not yet done so\n");
        exploration_report.push_str(&format!("2. If one of the above ancestor types looks like the correct syntax node, use it with `{}` to stage an edit operation.\n", &self.anchor_text));
        exploration_report.push_str("3. Review the diff\n");
        exploration_report.push_str(
            "4. Commit the edit to disk if it looks good, otherwise stage a different edit\n",
        );

        // Return exploration results as an error (this prevents actual editing)
        exploration_report
    }
}

/// Information about an anchor point found in the text
#[derive(Debug)]
struct AnchorInfo<'tree> {
    ancestor_chain: Vec<Ancestor>,
    found_target: bool,
    target_node: Option<Node<'tree>>,
    anchor_node: Node<'tree>,
}

#[derive(Debug, Clone)]
struct Ancestor {
    kind: String,
    ast: String,
    source: String,
}

impl std::fmt::Display for Ancestor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let Ancestor { kind, ast, source } = self;
        write!(
            f,
            "\n   • `{kind}`\n      -> AST: {ast}\n      -> SOURCE: `{source}`\n"
        )
    }
}

impl Ancestor {
    fn from_node(node: Node<'_>, source_code: &str) -> Self {
        let source = truncate(
            source_code[node.byte_range()]
                .to_string()
                .replace('\n', "\\n"),
            150,
        );
        let ast = truncate(node.to_string(), 150);
        Self {
            kind: node.kind().to_string(),
            ast,
            source,
        }
    }
}

/// Find the tree-sitter node at a specific byte position
fn find_node_at_byte_position<'a>(
    tree: &'a Tree,
    byte_start: usize,
    anchor_len: usize,
) -> Option<Node<'a>> {
    let root = tree.root_node();

    // Find the smallest node that contains this byte position
    let mut current = root;

    loop {
        let mut found_child = false;
        for i in 0..current.child_count() {
            if let Some(child) = current.child(i) {
                if child.start_byte() <= byte_start && (byte_start + anchor_len) <= child.end_byte()
                {
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

    // Check the current node first
    if current.kind() == target_type {
        return Some(current);
    }

    while let Some(parent) = current.parent() {
        if parent.kind() == target_type {
            return Some(parent);
        }
        current = parent;
    }

    None
}

fn truncate(mut s: String, len: usize) -> String {
    if s.len() > len {
        let len = s[..len].rfind(|c: char| !c.is_whitespace()).unwrap_or(len);
        s.truncate(len);
        s.push_str("...");
    }
    s
}

/// Get the chain of ancestor node types for error reporting
fn get_ancestor_chain(node: &Node, source_code: &str) -> Vec<Ancestor> {
    let mut chain = Vec::new();
    let mut current = *node;

    // Include the current node's type first
    chain.push(current);

    while let Some(parent) = current.parent() {
        chain.push(parent);
        current = parent;
    }

    chain
        .into_iter()
        .map(|node| Ancestor::from_node(node, source_code))
        .collect()
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
Suggestion: use the open_file tool if you have not yet done so for this file
"
        );

        // Show context for each anchor position
        for (i, info) in anchor_info.iter().enumerate() {
            message.push_str(&format!("\n- Occurence {}: ", i + 1));
            if let Some(context) =
                get_context_around_position(&info.anchor_node, source_code, anchor_text, 40)
            {
                message.push_str(&format!("`{context}`"));
            }

            if !info.ancestor_chain.is_empty() {
                let mut available_ancestors = info
                    .ancestor_chain
                    .iter()
                    .map(|ancestor| &*ancestor.kind)
                    .collect::<Vec<_>>();
                available_ancestors.dedup();

                message.push_str(&format!(
                    "\n   Available ancestors: {}",
                    available_ancestors.join(", ")
                ));
            }
        }

        let all_ancestors = anchor_info
            .iter()
            .flat_map(|info| info.ancestor_chain.iter().map(|x| x.kind.clone()))
            .collect();

        if let Some(suggestion) = suggest_ancestor_type(all_ancestors, ancestor_node_type) {
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
        "anchor_text `{anchor_text}` with ancestor_node_type `{ancestor_node_type}` matches {valid_count} nodes\nOccurrences:\n"
    );

    for info in anchor_info.iter().filter(|info| info.found_target) {
        if let Some(target_node) = &info.target_node {
            message.push_str(
                &truncate(source_code[target_node.byte_range()].to_string(), 50)
                    .replace('\n', "\\n")
                    .replace('\t', "\\t"),
            );
            message.push_str("\n\n");
        }
    }

    message.push_str(
        "Suggestions: 
-> Use more specific anchor text to distinguish between matches
-> use the open_file tool if you have not yet done so",
    );
    message
}

/// Get text context around a byte position
fn get_context_around_position(
    node: &Node<'_>,
    source_code: &str,
    anchor_text: &str,
    context_chars: usize,
) -> Option<String> {
    let node_text = &source_code[node.byte_range()];
    if node_text.len() > anchor_text.len() {
        return Some(node_text.to_string());
    }

    if let Some(parent) = node.parent() {
        let Range { start, end } = parent.byte_range();
        if end - start < context_chars {
            return Some(
                source_code[start..end]
                    .trim()
                    .replace('\n', "\\n")
                    .replace('\t', "\\t"),
            );
        }
    }

    let byte_pos = node.start_byte();
    let mut start = byte_pos.saturating_sub(context_chars);
    let mut end = (byte_pos + anchor_text.len() + context_chars).min(source_code.len());
    if let Some(second_anchor) =
        source_code[(byte_pos + anchor_text.len()).min(end)..end].find(anchor_text)
    {
        end = byte_pos + anchor_text.len() + second_anchor;
    }

    if let Some(earlier_anchor) = source_code[start..byte_pos].rfind(anchor_text) {
        start = start + earlier_anchor + anchor_text.len()
    }

    if start < source_code.len() && end <= source_code.len() {
        Some(
            source_code[start..end]
                .trim()
                .replace('\n', "\\n")
                .replace('\t', "\\t"),
        )
    } else {
        None
    }
}

/// Suggest a similar ancestor type based on fuzzy matching
fn suggest_ancestor_type(available: HashSet<String>, target: &str) -> Option<String> {
    let target_lower = target.to_lowercase();

    // Look for exact substring matches first
    for ancestor_kind in &available {
        if ancestor_kind.to_lowercase().contains(&target_lower) {
            return Some(ancestor_kind.clone());
        }
    }

    // Look for similar length matches
    for ancestor_kind in &available {
        if levenshtein_distance(&target_lower, &ancestor_kind.to_lowercase()) <= 2 {
            return Some(ancestor_kind.clone());
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
