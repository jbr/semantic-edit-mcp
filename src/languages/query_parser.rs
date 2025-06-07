use crate::languages::traits::{LanguageParser, NodeTypeInfo, LanguageQueries};
use anyhow::{Result, anyhow};
use tree_sitter::{Language, Node, Query, QueryCursor, StreamingIterator, Tree};

/// Generic query-based parser that works for any tree-sitter language
pub struct QueryBasedParser {
    language: Language,
    queries: LanguageQueries,
    node_types: Vec<NodeTypeInfo>,
}

impl QueryBasedParser {
    pub fn new(language: Language, queries: LanguageQueries, node_types: Vec<NodeTypeInfo>) -> Self {
        Self {
            language,
            queries,
            node_types,
        }
    }
    
    fn get_node_type_info(&self, node_type: &str) -> Result<&NodeTypeInfo> {
        self.node_types
            .iter()
            .find(|info| info.node_type == node_type)
            .ok_or_else(|| anyhow!("Unknown node type: {}", node_type))
    }
}

impl LanguageParser for QueryBasedParser {
    fn find_by_name<'a>(&self, tree: &'a Tree, source: &str, node_type: &str, name: &str) -> Result<Option<Node<'a>>> {
        let node_info = self.get_node_type_info(node_type)?;
        
        if !node_info.supports_search_by_name {
            return Err(anyhow!("Node type {} doesn't support name-based search", node_type));
        }
        
        // Build tree-sitter query dynamically based on node type metadata
        let query_text = if node_info.fields.contains(&"name".to_string()) {
            format!(r#"({node_type} name: (identifier) @name (#eq? @name "{name}")) @target"#)
        } else if node_info.fields.contains(&"key".to_string()) {
            // For JSON-like structures
            format!(r#"({node_type} key: (string) @key (#eq? @key '"{name}"')) @target"#)
        } else {
            return Err(anyhow!("Node type {} has no searchable name field", node_type));
        };
        
        let results = self.execute_query(&query_text, tree, source)?;
        Ok(results.into_iter().next())
    }
    
    fn find_by_type<'a>(&self, tree: &'a Tree, node_type: &str) -> Vec<Node<'a>> {
        let mut results = Vec::new();
        
        fn traverse<'a>(node: Node<'a>, node_type: &str, results: &mut Vec<Node<'a>>) {
            if node.kind() == node_type {
                results.push(node);
            }
            
            for child in node.children(&mut node.walk()) {
                traverse(child, node_type, results);
            }
        }
        
        traverse(tree.root_node(), node_type, &mut results);
        results
    }
    
    fn execute_query<'a>(&self, query_text: &str, tree: &'a Tree, source: &str) -> Result<Vec<Node<'a>>> {
        let query = Query::new(&self.language, query_text)?;
        let mut cursor = QueryCursor::new();
        let mut results = Vec::new();
        
        let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
        while let Some(m) = matches.next() {
            for capture in m.captures {
                results.push(capture.node);
            }
        }
        
        Ok(results)
    }
    
    fn validate_syntax(&self, source: &str) -> Result<bool> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&self.language)?;
        
        if let Some(tree) = parser.parse(source, None) {
            Ok(!tree.root_node().has_error())
        } else {
            Ok(false)
        }
    }
    
    fn get_all_names(&self, tree: &Tree, source: &str, node_type: &str) -> Vec<String> {
        let node_info = match self.get_node_type_info(node_type) {
            Ok(info) => info,
            Err(_) => return Vec::new(),
        };
        
        if !node_info.supports_search_by_name {
            return Vec::new();
        }
        
        // Build query to find all nodes of this type and extract their names
        let query_text = if node_info.fields.contains(&"name".to_string()) {
            format!(r#"({node_type} name: (identifier) @name)"#)
        } else if node_info.fields.contains(&"key".to_string()) {
            format!(r#"({node_type} key: (string) @key)"#)
        } else {
            return Vec::new();
        };
        
        if let Ok(query) = Query::new(&self.language, &query_text) {
            let mut cursor = QueryCursor::new();
            let mut names = Vec::new();
            
            let mut matches = cursor.matches(&query, tree.root_node(), source.as_bytes());
            while let Some(m) = matches.next() {
                for capture in m.captures {
                    let name_text = &source[capture.node.start_byte()..capture.node.end_byte()];
                    // For JSON, remove quotes from string literals
                    let clean_name = if name_text.starts_with('"') && name_text.ends_with('"') {
                        name_text[1..name_text.len()-1].to_string()
                    } else {
                        name_text.to_string()
                    };
                    names.push(clean_name);
                }
            }
            names
        } else {
            Vec::new()
        }
    }
}
