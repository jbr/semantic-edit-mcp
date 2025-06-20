use crate::languages::traits::NodeTypeInfo;
use anyhow::{anyhow, Result};
use serde_json::Value;
use tree_sitter::{Node, Tree};

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
    collect_errors_recursive(tree.root_node(), &mut errors);
    errors
}

fn collect_errors_recursive<'tree>(node: Node<'tree>, errors: &mut Vec<Node<'tree>>) {
    // Check if this node is an error
    if node.is_error() || node.kind() == "ERROR" {
        errors.push(node);
    }

    // Recursively check all children
    for child in node.children(&mut node.walk()) {
        collect_errors_recursive(child, errors);
    }
}
