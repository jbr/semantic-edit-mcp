use crate::languages::traits::NodeTypeInfo;
use anyhow::{anyhow, Result};
use serde_json::Value;
use tree_sitter::{Node, Tree, TreeCursor};

/// Parse node-types.json file from tree-sitter
pub fn parse_node_types_json(json_content: &str) -> Result<Vec<NodeTypeInfo>> {
    let node_types: Vec<Value> = serde_json::from_str(json_content)?;
    let mut result = Vec::new();

    for node_type_value in node_types {
        let obj = node_type_value
            .as_object()
            .ok_or_else(|| anyhow!("Expected object in node-types.json"))?;

        let node_type = obj
            .get("type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing 'type' field"))?
            .to_string();

        let named = obj.get("named").and_then(|v| v.as_bool()).unwrap_or(false);

        result.push(NodeTypeInfo::new(node_type, named));
    }

    Ok(result)
}

pub fn collect_errors<'tree>(tree: &'tree Tree) -> Vec<Node<'tree>> {
    let mut errors = vec![];
    let mut cursor = tree.root_node().walk();
    collect_errors_with_cursor(&mut cursor, &mut errors);
    errors
}

fn collect_errors_with_cursor<'tree>(
    cursor: &mut TreeCursor<'tree>,
    errors: &mut Vec<Node<'tree>>,
) {
    loop {
        let node = cursor.node();
        if node.kind() == "ERROR" {
            errors.push(node);
        }

        if cursor.goto_first_child() {
            collect_errors_with_cursor(cursor, errors);
            cursor.goto_parent();
        }

        if !cursor.goto_next_sibling() {
            break;
        }
    }
}
