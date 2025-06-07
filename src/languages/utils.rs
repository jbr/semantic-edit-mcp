use crate::languages::traits::NodeTypeInfo;
use anyhow::{Result, anyhow};
use serde_json::Value;

/// Parse node-types.json file from tree-sitter
pub fn parse_node_types_json(json_content: &str) -> Result<Vec<NodeTypeInfo>> {
    let node_types: Vec<Value> = serde_json::from_str(json_content)?;
    let mut result = Vec::new();
    
    for node_type_value in node_types {
        let obj = node_type_value.as_object()
            .ok_or_else(|| anyhow!("Expected object in node-types.json"))?;
        
        let node_type = obj.get("type")
            .and_then(|v| v.as_str())
            .ok_or_else(|| anyhow!("Missing 'type' field"))?
            .to_string();
        
        let named = obj.get("named")
            .and_then(|v| v.as_bool())
            .unwrap_or(true);
        
        // Extract field names from the fields object
        let fields = if let Some(fields_obj) = obj.get("fields").and_then(|v| v.as_object()) {
            fields_obj.keys().cloned().collect()
        } else {
            Vec::new()
        };
        
        result.push(NodeTypeInfo::new(node_type, named, fields));
    }
    
    Ok(result)
}

/// Load query files from disk
pub fn load_query_file(language: &tree_sitter::Language, file_path: &str) -> Result<Option<tree_sitter::Query>> {
    match std::fs::read_to_string(file_path) {
        Ok(content) => {
            let query = tree_sitter::Query::new(language, &content)?;
            Ok(Some(query))
        }
        Err(_) => Ok(None), // File doesn't exist, which is fine
    }
}
