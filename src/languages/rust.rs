use crate::languages::semantic_grouping::{GroupingRule, SemanticGrouping, WithSemanticGrouping};
use crate::languages::traits::{
    LanguageEditor, LanguageParser, LanguageQueries, LanguageSupport, NodeTypeInfo,
};
use crate::operations::{EditResult, NodeSelector};
use crate::parser::get_node_text;
use anyhow::{anyhow, Result};
use ropey::Rope;
use tree_sitter::{Language, Node, Query, StreamingIterator, Tree};

pub struct RustLanguage;

impl RustLanguage {
    pub fn new() -> Result<Self> {
        Ok(Self)
    }
}

impl LanguageSupport for RustLanguage {
    fn language_name(&self) -> &'static str {
        "rust"
    }

    fn file_extensions(&self) -> &'static [&'static str] {
        &["rs"]
    }

    fn tree_sitter_language(&self) -> Language {
        tree_sitter_rust::LANGUAGE.into()
    }

    fn get_node_types(&self) -> Result<Vec<NodeTypeInfo>> {
        Ok(vec![
            NodeTypeInfo::new(
                "function_item".to_string(),
                true,
                vec![
                    "name".to_string(),
                    "parameters".to_string(),
                    "body".to_string(),
                ],
            ),
            NodeTypeInfo::new(
                "struct_item".to_string(),
                true,
                vec!["name".to_string(), "body".to_string()],
            ),
            NodeTypeInfo::new(
                "impl_item".to_string(),
                true,
                vec!["type".to_string(), "body".to_string()],
            ),
            NodeTypeInfo::new(
                "mod_item".to_string(),
                true,
                vec!["name".to_string(), "body".to_string()],
            ),
        ])
    }

    fn load_queries(&self) -> Result<LanguageQueries> {
        let mut lq = LanguageQueries::new();
        lq.validation_queries = Some(Query::new(
            &self.tree_sitter_language(),
            include_str!("../../queries/rust/validation.scm"),
        )?);
        Ok(lq)
    }

    fn parser(&self) -> Box<dyn LanguageParser> {
        Box::new(RustParser::new())
    }

    fn editor(&self) -> Box<dyn LanguageEditor> {
        Box::new(RustEditor::new())
    }
}

impl SemanticGrouping for RustLanguage {
    fn get_grouping_rules(&self) -> Vec<GroupingRule> {
        vec![
            // Functions can have attributes and comments preceding them
            GroupingRule::new("function_item")
                .with_preceding_types(vec!["attribute_item", "line_comment", "block_comment"])
                .with_max_gap_nodes(2), // Allow some whitespace between elements
            // Structs can have attributes and comments
            GroupingRule::new("struct_item")
                .with_preceding_types(vec!["attribute_item", "line_comment", "block_comment"])
                .with_max_gap_nodes(2),
            // Enums can have attributes and comments
            GroupingRule::new("enum_item")
                .with_preceding_types(vec!["attribute_item", "line_comment", "block_comment"])
                .with_max_gap_nodes(2),
            // Impl blocks can have attributes and comments
            GroupingRule::new("impl_item")
                .with_preceding_types(vec!["attribute_item", "line_comment", "block_comment"])
                .with_max_gap_nodes(2),
            // Modules can have attributes and comments
            GroupingRule::new("mod_item")
                .with_preceding_types(vec!["attribute_item", "line_comment", "block_comment"])
                .with_max_gap_nodes(2),
        ]
    }
}

// WithSemanticGrouping is automatically implemented via blanket impl in semantic_grouping.rs

impl RustLanguage {
    /// Check if replacement content contains Rust-specific preceding elements
    pub fn replacement_has_preceding_elements(&self, content: &str) -> bool {
        let trimmed = content.trim_start();
        // Check for Rust attributes or comments at the start
        trimmed.starts_with('#') || trimmed.starts_with("//") || trimmed.starts_with("/*")
    }
}

impl WithSemanticGrouping for RustLanguage {
    fn replacement_has_preceding_elements(&self, content: &str) -> bool {
        self.replacement_has_preceding_elements(content)
    }
}

pub struct RustParser;
impl RustParser {
    pub fn new() -> Self {
        Self
    }
}

impl LanguageParser for RustParser {
    fn find_by_name<'a>(
        &self,
        tree: &'a Tree,
        source: &str,
        node_type: &str,
        name: &str,
    ) -> Result<Option<Node<'a>>> {
        // Implement Rust-specific name finding logic
        let root = tree.root_node();
        let mut cursor = root.walk();

        fn traverse_for_name<'a>(
            cursor: &mut tree_sitter::TreeCursor<'a>,
            source: &str,
            target_type: &str,
            target_name: &str,
        ) -> Option<Node<'a>> {
            let node = cursor.node();

            if node.kind() == target_type {
                // Check if this node has a name field that matches
                if let Some(name_node) = node.child_by_field_name("name") {
                    let name_text = get_node_text(&name_node, source);
                    if name_text == target_name {
                        return Some(node);
                    }
                }
            }

            if cursor.goto_first_child() {
                loop {
                    if let Some(found) = traverse_for_name(cursor, source, target_type, target_name)
                    {
                        return Some(found);
                    }
                    if !cursor.goto_next_sibling() {
                        break;
                    }
                }
                cursor.goto_parent();
            }

            None
        }

        Ok(traverse_for_name(&mut cursor, source, node_type, name))
    }

    fn find_by_type<'a>(&self, tree: &'a Tree, node_type: &str) -> Vec<Node<'a>> {
        let mut nodes = Vec::new();
        let mut cursor = tree.root_node().walk();

        fn traverse_for_type<'a>(
            cursor: &mut tree_sitter::TreeCursor<'a>,
            target_type: &str,
            results: &mut Vec<Node<'a>>,
        ) {
            let node = cursor.node();

            if node.kind() == target_type {
                results.push(node);
            }

            if cursor.goto_first_child() {
                loop {
                    traverse_for_type(cursor, target_type, results);
                    if !cursor.goto_next_sibling() {
                        break;
                    }
                }
                cursor.goto_parent();
            }
        }

        traverse_for_type(&mut cursor, node_type, &mut nodes);
        nodes
    }

    fn execute_query<'a>(
        &self,
        query_text: &str,
        tree: &'a Tree,
        source: &str,
    ) -> Result<Vec<Node<'a>>> {
        // Implement tree-sitter query execution
        let language = tree_sitter_rust::LANGUAGE.into();
        let query = tree_sitter::Query::new(&language, query_text)?;
        let mut cursor = tree_sitter::QueryCursor::new();

        let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
        let mut nodes = Vec::new();

        // Use StreamingIterator API to iterate over matches
        while let Some(match_) = matches.next() {
            for capture in match_.captures {
                nodes.push(capture.node);
            }
        }

        Ok(nodes)
    }

    fn validate_syntax(&self, source: &str) -> Result<bool> {
        validate_rust_syntax(source)
    }

    fn get_all_names(&self, tree: &Tree, source: &str, node_type: &str) -> Vec<String> {
        let nodes = self.find_by_type(tree, node_type);
        nodes
            .iter()
            .filter_map(|node| {
                node.child_by_field_name("name")
                    .map(|name_node| get_node_text(&name_node, source).to_string())
            })
            .collect()
    }
}

pub struct RustEditor {
    rust_language: RustLanguage,
}

impl RustEditor {
    pub fn new() -> Self {
        Self {
            rust_language: RustLanguage::new().expect("Failed to create Rust language support"),
        }
    }
}

impl LanguageEditor for RustEditor {
    fn format_code(&self, source: &str) -> Result<String> {
        // For now, just return the original code
        // In a full implementation, we'd integrate with rustfmt
        Ok(source.to_string())
    }

    /// Replace a node using semantic grouping to determine the appropriate range
    fn replace<'tree>(
        &self,
        node: Node<'tree>,
        tree: &Tree,
        source_code: &str,
        _selector: &NodeSelector,
        new_content: &str,
    ) -> Result<EditResult> {
        // Use semantic grouping to calculate the replacement range
        let (actual_start_byte, actual_end_byte) =
            self.rust_language
                .calculate_replacement_range(tree, node, new_content, source_code)?;

        // Validate the new content would create valid syntax
        if !self.validate_replacement_with_range(
            source_code,
            actual_start_byte,
            actual_end_byte,
            new_content,
        )? {
            return Ok(EditResult::Error(
                "Replacement would create invalid syntax".to_string(),
            ));
        }

        let rope = Rope::from_str(source_code);

        // Convert byte positions to character positions
        let start_char = rope.byte_to_char(actual_start_byte);
        let end_char = rope.byte_to_char(actual_end_byte);

        // Create new rope with replacement
        let mut new_rope = rope.clone();
        new_rope.remove(start_char..end_char);
        new_rope.insert(start_char, new_content);

        let message = format!("Successfully replaced {} node", node.kind());

        Ok(EditResult::Success {
            message,
            new_content: new_rope.to_string(),
            affected_range: (start_char, start_char + new_content.len()),
        })
    }

    /// Insert before a node using semantic grouping
    fn insert_before<'tree>(
        &self,
        node: Node<'tree>,
        tree: &Tree,
        source_code: &str,
        _selector: &NodeSelector,
        content: &str,
    ) -> Result<EditResult> {
        let (insert_pos, _) = self
            .rust_language
            .calculate_insertion_range(tree, node, true)?;

        let rope = Rope::from_str(source_code);
        let insert_char = rope.byte_to_char(insert_pos);

        // Find appropriate indentation
        let line_start = rope.line_to_char(rope.char_to_line(insert_char));
        let line_content = rope.slice(line_start..insert_char).to_string();
        let indentation = line_content
            .chars()
            .take_while(|c| c.is_whitespace())
            .collect::<String>();

        let content_with_newline = format!("{content}\n{indentation}");

        let mut new_rope = rope.clone();
        new_rope.insert(insert_char, &content_with_newline);

        let message = format!("Successfully inserted content before {} node", node.kind());

        Ok(EditResult::Success {
            message,
            new_content: new_rope.to_string(),
            affected_range: (insert_char, insert_char + content_with_newline.len()),
        })
    }

    /// Insert after a node using semantic grouping
    fn insert_after<'tree>(
        &self,
        node: Node<'tree>,
        tree: &Tree,
        source_code: &str,
        _selector: &NodeSelector,
        content: &str,
    ) -> Result<EditResult> {
        let (insert_pos, _) = self
            .rust_language
            .calculate_insertion_range(tree, node, false)?;

        let rope = Rope::from_str(source_code);
        let insert_char = rope.byte_to_char(insert_pos);

        // Find appropriate indentation by looking at the node's line
        let start_char = rope.byte_to_char(node.start_byte());
        let line_start = rope.line_to_char(rope.char_to_line(start_char));
        let line_content = rope.slice(line_start..start_char).to_string();
        let indentation = line_content
            .chars()
            .take_while(|c| c.is_whitespace())
            .collect::<String>();

        let content_with_newline = format!("\n{indentation}{content}");

        let mut new_rope = rope.clone();
        new_rope.insert(insert_char, &content_with_newline);

        let message = format!("Successfully inserted content after {} node", node.kind());

        Ok(EditResult::Success {
            message,
            new_content: new_rope.to_string(),
            affected_range: (insert_char, insert_char + content_with_newline.len()),
        })
    }

    /// Wrap a node using semantic grouping
    fn wrap<'tree>(
        &self,
        node: Node<'tree>,
        tree: &Tree,
        source_code: &str,
        _selector: &NodeSelector,
        wrapper_template: &str,
    ) -> Result<EditResult> {
        if !wrapper_template.contains("{{content}}") {
            return Err(anyhow!(
                "Wrapper template must contain {{content}} placeholder"
            ));
        }

        // Get the semantic group and its text
        let group = self.rust_language.find_semantic_group(tree, node)?;
        let (group_start, group_end) = group.byte_range();
        let group_text = &source_code[group_start..group_end];

        let wrapped_content = wrapper_template.replace("{{content}}", group_text);

        // Validate the wrapped content would create valid syntax
        if !self.validate_replacement_with_range(
            source_code,
            group_start,
            group_end,
            &wrapped_content,
        )? {
            return Ok(EditResult::Error(
                "Wrapping would create invalid syntax".to_string(),
            ));
        }

        let rope = Rope::from_str(source_code);
        let start_char = rope.byte_to_char(group_start);
        let end_char = rope.byte_to_char(group_end);

        let mut new_rope = rope.clone();
        new_rope.remove(start_char..end_char);
        new_rope.insert(start_char, &wrapped_content);

        let message = if group.has_preceding_elements() || group.has_following_elements() {
            format!(
                "Successfully wrapped {} group ({} total elements)",
                node.kind(),
                group.all_nodes().len()
            )
        } else {
            format!("Successfully wrapped {} node", node.kind())
        };

        Ok(EditResult::Success {
            message,
            new_content: new_rope.to_string(),
            affected_range: (start_char, start_char + wrapped_content.len()),
        })
    }

    /// Delete a node using semantic grouping
    fn delete(
        &self,
        node: Node,
        tree: &Tree,
        source_code: &str,
        _selector: &NodeSelector,
    ) -> Result<EditResult> {
        // Get the semantic group
        let group = self.rust_language.find_semantic_group(tree, node)?;
        let (group_start, group_end) = group.byte_range();

        let rope = Rope::from_str(source_code);
        let start_char = rope.byte_to_char(group_start);
        let end_char = rope.byte_to_char(group_end);

        let mut new_rope = rope.clone();
        new_rope.remove(start_char..end_char);

        let message = if group.has_preceding_elements() || group.has_following_elements() {
            format!(
                "Successfully deleted {} group ({} total elements)",
                node.kind(),
                group.all_nodes().len()
            )
        } else {
            format!("Successfully deleted {} node", node.kind())
        };

        Ok(EditResult::Success {
            message,
            new_content: new_rope.to_string(),
            affected_range: (start_char, start_char),
        })
    }

    fn validate_replacement(&self, original: &str, node: &Node, replacement: &str) -> Result<bool> {
        // Create a temporary version with the replacement
        let rope = Rope::from_str(original);
        let start_char = rope.byte_to_char(node.start_byte());
        let end_char = rope.byte_to_char(node.end_byte());

        let mut temp_rope = rope.clone();
        temp_rope.remove(start_char..end_char);
        temp_rope.insert(start_char, replacement);

        let temp_code = temp_rope.to_string();

        // Parse and check for syntax errors
        validate_rust_syntax(&temp_code)
    }
}

impl RustEditor {
    /// Validate replacement with custom byte range
    fn validate_replacement_with_range(
        &self,
        original_code: &str,
        start_byte: usize,
        end_byte: usize,
        replacement: &str,
    ) -> Result<bool> {
        let rope = Rope::from_str(original_code);
        let start_char = rope.byte_to_char(start_byte);
        let end_char = rope.byte_to_char(end_byte);

        let mut temp_rope = rope.clone();
        temp_rope.remove(start_char..end_char);
        temp_rope.insert(start_char, replacement);

        let temp_code = temp_rope.to_string();
        validate_rust_syntax(&temp_code)
    }
}

fn validate_rust_syntax(source_code: &str) -> Result<bool> {
    let mut parser = tree_sitter::Parser::new();
    parser.set_language(&tree_sitter_rust::LANGUAGE.into())?;

    if let Some(tree) = parser.parse(source_code, None) {
        Ok(!tree.root_node().has_error())
    } else {
        Ok(false)
    }
}
