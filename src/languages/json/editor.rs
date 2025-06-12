use std::{borrow::Cow, collections::BTreeMap};

use crate::languages::{traits::LanguageEditor, utils::collect_errors};
use crate::operations::EditResult;
use crate::parser::get_node_text;
use anyhow::{anyhow, Result};
use jsonformat::Indentation;
use ropey::Rope;
use tree_sitter::{Node, Tree};

pub struct JsonEditor;

impl Default for JsonEditor {
    fn default() -> Self {
        Self::new()
    }
}

impl JsonEditor {
    pub fn new() -> Self {
        Self
    }

    fn adjust_deletion_range_for_comma(
        rope: &Rope,
        start_char: usize,
        end_char: usize,
        node: &Node,
    ) -> (usize, usize) {
        // If deleting a pair, also remove associated comma
        if let Some(next_sibling) = node.next_sibling() {
            if next_sibling.kind() == "," {
                let comma_end = rope.byte_to_char(next_sibling.end_byte());
                return (start_char, comma_end);
            }
        }

        if let Some(prev_sibling) = node.prev_sibling() {
            if prev_sibling.kind() == "," {
                let comma_start = rope.byte_to_char(prev_sibling.start_byte());
                return (comma_start, end_char);
            }
        }

        (start_char, end_char)
    }
}

impl LanguageEditor for JsonEditor {
    fn format_code(&self, source: &str) -> Result<String> {
        let mut tab_count = 0;
        let mut space_counts = BTreeMap::<usize, usize>::new();
        let mut last_indentation = 0;
        let mut last_change = 0;
        for line in source.lines().take(100) {
            if line.starts_with('\t') {
                tab_count += 1;
            } else {
                let count = line.chars().take_while(|c| c == &' ').count();
                let diff = count.abs_diff(last_indentation);
                last_indentation = count;
                if diff > 0 {
                    last_change = diff;
                }
                let entry = space_counts.entry(last_change).or_default();
                *entry += 1;
            }
        }

        let custom;

        let indentation_style = match space_counts
            .into_iter()
            .map(|(k, v)| (Some(k), v))
            .chain(std::iter::once((None, tab_count)))
            .max_by_key(|(_, count)| *count)
        {
            Some((Some(2), _)) => Indentation::TwoSpace,
            Some((Some(4), _)) => Indentation::FourSpace,
            Some((None, _)) => Indentation::Tab,
            Some((Some(n), _)) => {
                custom = " ".repeat(n);
                Indentation::Custom(&custom)
            }
            None => Indentation::FourSpace,
        };

        Ok(jsonformat::format(source, indentation_style))
    }

    fn replace<'tree>(
        &self,
        node: Node<'tree>,
        _tree: &Tree,
        source_code: &str,
        new_content: &str,
    ) -> Result<EditResult> {
        let mut rope = Rope::from_str(source_code);
        let start_byte = node.start_byte();
        let end_byte = node.end_byte();

        let start_char = rope.byte_to_char(start_byte);
        let end_char = rope.byte_to_char(end_byte);

        rope.remove(start_char..end_char);
        rope.insert(start_char, new_content);

        Ok(EditResult::Success {
            message: format!("Successfully replaced {} node", node.kind()),
            new_content: rope.to_string(),
        })
    }

    fn insert_before<'tree>(
        &self,
        node: Node<'tree>,
        _tree: &Tree,
        source_code: &str,
        content: &str,
    ) -> Result<EditResult> {
        let mut rope = Rope::from_str(source_code);
        let start_byte = node.start_byte();
        let start_char = rope.byte_to_char(start_byte);

        let mut content = Cow::Borrowed(content);

        let trimmed = content.trim_end();
        if !trimmed.ends_with(',') {
            content = Cow::Owned(format!("{trimmed},"));
        }

        rope.insert(start_char, &content);

        Ok(EditResult::Success {
            message: format!("Successfully inserted content before {} node", node.kind()),
            new_content: rope.to_string(),
        })
    }

    fn insert_after<'tree>(
        &self,
        node: Node<'tree>,
        _tree: &Tree,
        source_code: &str,
        content: &str,
    ) -> Result<EditResult> {
        let mut rope = Rope::from_str(source_code);
        let end_byte = node.end_byte();
        let end_char = rope.byte_to_char(end_byte);

        rope.insert(end_char, content);
        if !content.trim_start().starts_with(',') {
            rope.insert_char(end_char, ',');
        }

        Ok(EditResult::Success {
            message: format!("Successfully inserted content after {} node", node.kind()),
            new_content: rope.to_string(),
        })
    }

    fn wrap<'tree>(
        &self,
        node: Node<'tree>,
        _tree: &Tree,
        source_code: &str,
        wrapper_template: &str,
    ) -> Result<EditResult> {
        let node_text = get_node_text(&node, source_code);

        if !wrapper_template.contains("{{content}}") {
            return Err(anyhow!(
                "Wrapper template must contain {{content}} placeholder"
            ));
        }

        let wrapped_content = wrapper_template.replace("{{content}}", node_text);

        let mut rope = Rope::from_str(source_code);
        let start_byte = node.start_byte();
        let end_byte = node.end_byte();
        let start_char = rope.byte_to_char(start_byte);
        let end_char = rope.byte_to_char(end_byte);

        rope.remove(start_char..end_char);
        rope.insert(start_char, &wrapped_content);

        Ok(EditResult::Success {
            message: format!("Successfully wrapped {} node", node.kind()),
            new_content: rope.to_string(),
        })
    }

    fn delete<'tree>(
        &self,
        node: Node<'tree>,
        _tree: &Tree,
        source_code: &str,
    ) -> Result<EditResult> {
        let mut rope = Rope::from_str(source_code);
        let start_byte = node.start_byte();
        let end_byte = node.end_byte();
        let start_char = rope.byte_to_char(start_byte);
        let end_char = rope.byte_to_char(end_byte);

        // Handle comma removal for JSON objects/arrays
        let (final_start, final_end) = if node.kind() == "pair" {
            Self::adjust_deletion_range_for_comma(&rope, start_char, end_char, &node)
        } else {
            (start_char, end_char)
        };

        rope.remove(final_start..final_end);

        Ok(EditResult::Success {
            message: format!("Successfully deleted {} node", node.kind()),
            new_content: rope.to_string(),
        })
    }

    fn collect_errors(&self, _tree: &Tree, content: &str) -> Vec<usize> {
        match serde_json::from_str::<serde_json::Value>(content) {
            Ok(_) => vec![],
            Err(e) => {
                vec![e.line().saturating_sub(1)]
            }
        }
    }
}
