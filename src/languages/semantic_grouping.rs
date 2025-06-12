use anyhow::Result;
use tree_sitter::{Node, Tree};

/// Represents a rule for grouping related AST nodes that should be treated as a logical unit
#[derive(Debug, Clone)]
pub struct GroupingRule {
    /// The primary node type that this rule applies to (e.g., "function_item")
    pub target_node_type: String,

    /// Types of nodes that can precede the target and be considered part of the group
    /// e.g., ["attribute_item", "line_comment", "block_comment"]
    pub preceding_node_types: Vec<String>,

    /// Types of nodes that can follow the target and be considered part of the group
    /// (less common, but useful for some languages)
    pub following_node_types: Vec<String>,

    /// Maximum number of non-matching siblings to skip when looking for preceding nodes
    /// This handles cases where there might be whitespace or other separator nodes
    pub max_gap_nodes: usize,

    /// Whether to include only consecutive preceding nodes or all matching ones
    pub require_consecutive: bool,
}

impl GroupingRule {
    pub fn new(target_node_type: impl Into<String>) -> Self {
        Self {
            target_node_type: target_node_type.into(),
            preceding_node_types: Vec::new(),
            following_node_types: Vec::new(),
            max_gap_nodes: 0,
            require_consecutive: true,
        }
    }

    pub fn with_preceding_types(mut self, types: Vec<impl Into<String>>) -> Self {
        self.preceding_node_types = types.into_iter().map(|t| t.into()).collect();
        self
    }

    pub fn with_following_types(mut self, types: Vec<impl Into<String>>) -> Self {
        self.following_node_types = types.into_iter().map(|t| t.into()).collect();
        self
    }

    pub fn with_max_gap_nodes(mut self, max_gap: usize) -> Self {
        self.max_gap_nodes = max_gap;
        self
    }

    pub fn allow_non_consecutive(mut self) -> Self {
        self.require_consecutive = false;
        self
    }
}

/// A semantic group representing a logical unit of related AST nodes
#[derive(Debug)]
pub struct SemanticGroup<'a> {
    /// The primary/target node
    pub primary_node: Node<'a>,

    /// Nodes that precede the primary node and are logically part of the group
    pub preceding_nodes: Vec<Node<'a>>,

    /// Nodes that follow the primary node and are logically part of the group
    pub following_nodes: Vec<Node<'a>>,
}

impl<'a> SemanticGroup<'a> {
    pub fn new(primary_node: Node<'a>) -> Self {
        Self {
            primary_node,
            preceding_nodes: Vec::new(),
            following_nodes: Vec::new(),
        }
    }

    /// Get the byte range that encompasses the entire semantic group
    pub fn byte_range(&self) -> (usize, usize) {
        let start = self
            .preceding_nodes
            .first()
            .map(|n| n.start_byte())
            .unwrap_or(self.primary_node.start_byte());

        let end = self
            .following_nodes
            .last()
            .map(|n| n.end_byte())
            .unwrap_or(self.primary_node.end_byte());

        (start, end)
    }

    /// Get all nodes in the group in source order
    pub fn all_nodes(&self) -> Vec<Node<'a>> {
        let mut nodes = Vec::new();
        nodes.extend(self.preceding_nodes.iter());
        nodes.push(self.primary_node);
        nodes.extend(self.following_nodes.iter());
        nodes
    }

    /// Check if the group contains any preceding elements
    pub fn has_preceding_elements(&self) -> bool {
        !self.preceding_nodes.is_empty()
    }

    /// Check if the group contains any following elements  
    pub fn has_following_elements(&self) -> bool {
        !self.following_nodes.is_empty()
    }
}

/// Trait for language-specific semantic grouping logic
pub trait SemanticGrouping {
    /// Get the grouping rules for this language
    fn get_grouping_rules(&self) -> Vec<GroupingRule>;

    /// Find the semantic group for a given node
    fn find_semantic_group<'a>(&self, tree: &'a Tree, node: Node<'a>) -> Result<SemanticGroup<'a>> {
        let rules = self.get_grouping_rules();

        // Find applicable rule for this node type
        let applicable_rule = rules
            .iter()
            .find(|rule| rule.target_node_type == node.kind());

        let Some(rule) = applicable_rule else {
            // No grouping rule for this node type, return just the node itself
            return Ok(SemanticGroup::new(node));
        };

        let mut group = SemanticGroup::new(node);

        // Find preceding nodes
        if !rule.preceding_node_types.is_empty() {
            group.preceding_nodes = self.find_preceding_nodes(tree, node, rule)?;
        }

        // Find following nodes
        if !rule.following_node_types.is_empty() {
            group.following_nodes = self.find_following_nodes(tree, node, rule)?;
        }

        Ok(group)
    }

    /// Find nodes that precede the target node and should be grouped with it
    fn find_preceding_nodes<'a>(
        &self,
        _tree: &'a Tree,
        target_node: Node<'a>,
        rule: &GroupingRule,
    ) -> Result<Vec<Node<'a>>> {
        let Some(parent) = target_node.parent() else {
            return Ok(Vec::new());
        };

        let mut preceding = Vec::new();
        let mut cursor = parent.walk();

        // Navigate to our target node
        cursor.goto_first_child();
        while cursor.node().id() != target_node.id() {
            if !cursor.goto_next_sibling() {
                return Ok(Vec::new()); // Couldn't find target node
            }
        }

        // Now walk backwards to find preceding nodes
        let mut gap_count = 0;

        while cursor.goto_previous_sibling() {
            let current_node = cursor.node();
            let is_matching = rule
                .preceding_node_types
                .contains(&current_node.kind().to_string());

            if is_matching {
                preceding.push(current_node);
                gap_count = 0; // Reset gap count
            } else if self.is_gap_node(&current_node) {
                gap_count += 1;
                if gap_count > rule.max_gap_nodes {
                    if rule.require_consecutive {
                        break;
                    }
                }
            } else {
                // Hit a non-matching, non-gap node
                if rule.require_consecutive {
                    break;
                }
                gap_count += 1;
                if gap_count > rule.max_gap_nodes {
                    break;
                }
            }
        }

        // Reverse to get proper source order
        preceding.reverse();
        Ok(preceding)
    }

    /// Find nodes that follow the target node and should be grouped with it
    fn find_following_nodes<'a>(
        &self,
        _tree: &'a Tree,
        target_node: Node<'a>,
        rule: &GroupingRule,
    ) -> Result<Vec<Node<'a>>> {
        let Some(parent) = target_node.parent() else {
            return Ok(Vec::new());
        };

        let mut following = Vec::new();
        let mut cursor = parent.walk();

        // Navigate to our target node
        cursor.goto_first_child();
        while cursor.node().id() != target_node.id() {
            if !cursor.goto_next_sibling() {
                return Ok(Vec::new()); // Couldn't find target node
            }
        }

        // Now walk forwards to find following nodes
        let mut gap_count = 0;

        while cursor.goto_next_sibling() {
            let current_node = cursor.node();
            let is_matching = rule
                .following_node_types
                .contains(&current_node.kind().to_string());

            if is_matching {
                following.push(current_node);
                gap_count = 0; // Reset gap count
            } else if self.is_gap_node(&current_node) {
                gap_count += 1;
                if gap_count > rule.max_gap_nodes {
                    if rule.require_consecutive {
                        break;
                    }
                }
            } else {
                // Hit a non-matching, non-gap node
                if rule.require_consecutive {
                    break;
                }
                gap_count += 1;
                if gap_count > rule.max_gap_nodes {
                    break;
                }
            }
        }

        Ok(following)
    }

    /// Determine if a node is a "gap" node (like whitespace) that doesn't break grouping
    fn is_gap_node(&self, node: &Node) -> bool {
        // Common gap node types across languages
        matches!(node.kind(), "whitespace" | "newline" | "\n" | " " | "\t")
    }

    /// Validate that content between two nodes only contains allowed gap content
    fn validate_gap_content(&self, source_code: &str, start_byte: usize, end_byte: usize) -> bool {
        let gap_content = &source_code[start_byte..end_byte];
        gap_content.trim().is_empty() // Only whitespace is allowed
    }
}

/// Helper trait to add semantic grouping capabilities to existing language implementations
pub trait WithSemanticGrouping: SemanticGrouping {
    /// Calculate the appropriate range for a replacement operation, considering semantic grouping
    fn calculate_replacement_range<'a>(
        &self,
        tree: &'a Tree,
        node: Node<'a>,
        replacement_content: &str,
        _source_code: &str,
    ) -> Result<(usize, usize)> {
        let group = self.find_semantic_group(tree, node)?;

        // Determine if replacement content contains the same types of preceding elements
        let replacement_has_attributes =
            self.replacement_has_preceding_elements(replacement_content);

        if replacement_has_attributes && group.has_preceding_elements() {
            // Both old and new have preceding elements - replace the whole group
            Ok(group.byte_range())
        } else if !replacement_has_attributes && group.has_preceding_elements() {
            // Old has preceding elements but new doesn't - preserve the old ones by only replacing the primary node
            Ok((
                group.primary_node.start_byte(),
                group.primary_node.end_byte(),
            ))
        } else if replacement_has_attributes && !group.has_preceding_elements() {
            // New has preceding elements but old doesn't - just replace the primary node
            // (the new preceding elements will be part of the replacement)
            Ok((
                group.primary_node.start_byte(),
                group.primary_node.end_byte(),
            ))
        } else {
            // Neither has preceding elements - normal replacement
            Ok((
                group.primary_node.start_byte(),
                group.primary_node.end_byte(),
            ))
        }
    }

    /// Check if replacement content contains preceding elements (like attributes/comments)
    /// This should be implemented per language
    fn replacement_has_preceding_elements(&self, content: &str) -> bool;

    /// Calculate range for insertion operations, considering semantic grouping
    fn calculate_insertion_range<'a>(
        &self,
        tree: &'a Tree,
        node: Node<'a>,
        insert_before: bool,
    ) -> Result<(usize, usize)> {
        let group = self.find_semantic_group(tree, node)?;

        if insert_before {
            // Insert before the entire group (including any preceding elements)
            let start = group
                .preceding_nodes
                .first()
                .map(|n| n.start_byte())
                .unwrap_or(group.primary_node.start_byte());
            Ok((start, start))
        } else {
            // Insert after the entire group (including any following elements)
            let end = group
                .following_nodes
                .last()
                .map(|n| n.end_byte())
                .unwrap_or(group.primary_node.end_byte());
            Ok((end, end))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestGrouping;

    impl SemanticGrouping for TestGrouping {
        fn get_grouping_rules(&self) -> Vec<GroupingRule> {
            vec![GroupingRule::new("function_item")
                .with_preceding_types(vec!["attribute_item", "line_comment"])
                .with_max_gap_nodes(1)]
        }
    }

    #[test]
    fn test_grouping_rule_builder() {
        let rule = GroupingRule::new("function_item")
            .with_preceding_types(vec!["attribute_item", "line_comment"])
            .with_following_types(vec!["block_comment"])
            .with_max_gap_nodes(2)
            .allow_non_consecutive();

        assert_eq!(rule.target_node_type, "function_item");
        assert_eq!(
            rule.preceding_node_types,
            vec!["attribute_item", "line_comment"]
        );
        assert_eq!(rule.following_node_types, vec!["block_comment"]);
        assert_eq!(rule.max_gap_nodes, 2);
        assert!(!rule.require_consecutive);
    }
}
