use serde::{Deserialize, Serialize};
use anyhow::{Result, anyhow};
use tree_sitter::Node;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum NodeSelector {
    #[serde(rename = "name")]
    ByName { 
        node_type: Option<String>,
        name: String 
    },
    #[serde(rename = "type")]
    ByType { 
        node_type: String 
    },
    #[serde(rename = "query")]
    ByQuery { 
        query: String 
    },
    #[serde(rename = "position")]
    ByPosition { 
        line: usize, 
        column: usize,
        /// Optional scope hint: "token" (default), "expression", "statement", "item"
        #[serde(default)]
        scope: Option<String>,
    },
}

/// Find an ancestor node of one of the specified types
fn find_ancestor_of_type<'a>(node: &Node<'a>, target_types: &[&str]) -> Option<Node<'a>> {
    let mut current = *node;
    while let Some(parent) = current.parent() {
        if target_types.contains(&parent.kind()) {
            return Some(parent);
        }
        current = parent;
    }
    None
}

#[derive(Debug, Clone)]
pub enum EditOperation {
    Replace {
        target: NodeSelector,
        new_content: String,
    },
    InsertBefore {
        target: NodeSelector,
        content: String,
    },
    InsertAfter {
        target: NodeSelector,
        content: String,
    },
    Wrap {
        target: NodeSelector,
        wrapper_template: String,
    },
    Delete {
        target: NodeSelector,
    },
}

#[derive(Debug)]
pub struct EditResult {
    pub success: bool,
    pub message: String,
    pub new_content: Option<String>,
    pub affected_range: Option<(usize, usize)>,
}

impl EditOperation {
    pub fn apply(&self, source_code: &str, language: &str) -> Result<EditResult> {
        // This will be implemented in the specific language editors
        match language {
            "rust" => crate::editors::rust::RustEditor::apply_operation(self, source_code),
            _ => Err(anyhow!("Unsupported language for editing: {}", language)),
        }
    }
}

impl NodeSelector {
    pub fn find_node<'a>(&self, tree: &'a tree_sitter::Tree, source_code: &str, language: &str) -> Result<Option<Node<'a>>> {
        match self {
            NodeSelector::ByPosition { line, column, scope } => {
                let node = crate::parsers::find_node_by_position(tree, *line, *column);
                if let Some(node) = node {
                    // Apply scope-based filtering if requested
                    let final_node = match scope.as_deref() {
                        Some("expression") => find_ancestor_of_type(&node, &["expression_statement", "call_expression", "macro_invocation"]),
                        Some("statement") => find_ancestor_of_type(&node, &["expression_statement", "let_declaration", "item_declaration"]),
                        Some("item") => find_ancestor_of_type(&node, &["function_item", "struct_item", "impl_item", "mod_item"]),
                        Some("token") | None => Some(node), // Default behavior
                        _ => Some(node), // Unknown scope, use default
                    };
                    Ok(final_node)
                } else {
                    Ok(None)
                }
            },
            NodeSelector::ByName { node_type, name } => {
                match language {
                    "rust" => {
                        if let Some(nt) = node_type {
                            match nt.as_str() {
                                "function_item" => crate::parsers::rust::RustParser::find_function_by_name(tree, source_code, name),
                                "struct_item" => crate::parsers::rust::RustParser::find_struct_by_name(tree, source_code, name),
                                _ => Err(anyhow!("Unsupported node type for name search: {}", nt)),
                            }
                        } else {
                            // Try to find by name in any context - this is more complex
                            // For now, try function first, then struct
                            if let Ok(Some(node)) = crate::parsers::rust::RustParser::find_function_by_name(tree, source_code, name) {
                                Ok(Some(node))
                            } else {
                                crate::parsers::rust::RustParser::find_struct_by_name(tree, source_code, name)
                            }
                        }
                    },
                    _ => Err(anyhow!("Unsupported language for name search: {}", language)),
                }
            },
            NodeSelector::ByType { node_type } => {
                match language {
                    "rust" => {
                        let nodes = crate::parsers::rust::RustParser::find_nodes_by_type(tree, node_type);
                        Ok(nodes.into_iter().next())
                    },
                    _ => Err(anyhow!("Unsupported language for type search: {}", language)),
                }
            },
            NodeSelector::ByQuery { query } => {
                // Generic tree-sitter query execution
                self.execute_query(tree, source_code, language, query)
            },
        }
    }

    fn execute_query<'a>(&self, tree: &'a tree_sitter::Tree, source_code: &str, language: &str, query_text: &str) -> Result<Option<Node<'a>>> {
        let language_obj = match language {
            "rust" => tree_sitter_rust::LANGUAGE.into(),
            _ => return Err(anyhow!("Unsupported language for queries: {}", language)),
        };

        let query = tree_sitter::Query::new(&language_obj, query_text)?;
        let mut cursor = tree_sitter::QueryCursor::new();
        
        for m in cursor.matches(&query, tree.root_node(), source_code.as_bytes()) {
            if let Some(capture) = m.captures.first() {
                return Ok(Some(capture.node));
            }
        }
        
        Ok(None)
    }
}
