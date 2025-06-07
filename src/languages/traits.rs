use anyhow::Result;
use std::collections::HashMap;
use tree_sitter::{Language, Node, Query, Tree};

/// Core trait that all language implementations must provide
pub trait LanguageSupport: Send + Sync {
    fn language_name(&self) -> &'static str;
    fn file_extensions(&self) -> &'static [&'static str];
    fn tree_sitter_language(&self) -> Language;
    
    /// Load node types from the tree-sitter generated node-types.json
    fn get_node_types(&self) -> Result<Vec<NodeTypeInfo>>;
    
    /// Load tree-sitter query files for this language
    fn load_queries(&self) -> Result<LanguageQueries>;
    
    /// Get a parser instance for this language
    fn parser(&self) -> Box<dyn LanguageParser>;
    
    /// Get an editor instance for this language
    fn editor(&self) -> Box<dyn LanguageEditor>;
}

/// Information about a node type from tree-sitter's node-types.json
#[derive(Debug, Clone)]
pub struct NodeTypeInfo {
    pub node_type: String,        // from node-types.json: "function_item", "object"
    pub named: bool,              // from node-types.json
    pub fields: Vec<String>,      // from node-types.json: field names like "name", "body"
    pub supports_search_by_name: bool,  // derived: has "name" field?
    pub display_name: String,     // human-readable: "Function", "JSON Object"
}

impl NodeTypeInfo {
    pub fn new(node_type: String, named: bool, fields: Vec<String>) -> Self {
        let supports_search_by_name = fields.contains(&"name".to_string()) 
            || fields.contains(&"key".to_string());
        
        let display_name = match node_type.as_str() {
            "function_item" => "Function".to_string(),
            "struct_item" => "Struct".to_string(),
            "impl_item" => "Implementation".to_string(),
            "enum_item" => "Enum".to_string(),
            "mod_item" => "Module".to_string(),
            "object" => "JSON Object".to_string(),
            "array" => "JSON Array".to_string(),
            "pair" => "JSON Property".to_string(),
            "table" => "TOML Table".to_string(),
            "atx_heading" => "Markdown Heading".to_string(),
            "fenced_code_block" => "Code Block".to_string(),
            _ => {
                // Convert snake_case to Title Case
                node_type.split('_')
                    .map(|word| {
                        let mut chars = word.chars();
                        match chars.next() {
                            None => String::new(),
                            Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                        }
                    })
                    .collect::<Vec<_>>()
                    .join(" ")
            }
        };
        
        Self {
            node_type,
            named,
            fields,
            supports_search_by_name,
            display_name,
        }
    }
}

/// Collection of tree-sitter queries for a language
#[derive(Debug)]
pub struct LanguageQueries {
    pub highlights: Option<Query>,
    pub locals: Option<Query>, 
    pub tags: Option<Query>,
    pub operations: Option<Query>,    // NEW: for semantic editing operations
    pub custom_queries: HashMap<String, Query>,
}

impl LanguageQueries {
    pub fn new() -> Self {
        Self {
            highlights: None,
            locals: None,
            tags: None,
            operations: None,
            custom_queries: HashMap::new(),
        }
    }
}

/// Trait for language-specific parsing operations
pub trait LanguageParser: Send + Sync {
    /// Find a node by name (for nodes that have a "name" field)
    fn find_by_name<'a>(&self, tree: &'a Tree, source: &str, node_type: &str, name: &str) -> Result<Option<Node<'a>>>;
    
    /// Find nodes by type
    fn find_by_type<'a>(&self, tree: &'a Tree, node_type: &str) -> Vec<Node<'a>>;
    
    /// Execute a custom tree-sitter query
    fn execute_query<'a>(&self, query_text: &str, tree: &'a Tree, source: &str) -> Result<Vec<Node<'a>>>;
    
    /// Validate syntax for this language
    fn validate_syntax(&self, source: &str) -> Result<bool>;
    
    /// Get all names for a specific node type (e.g., all function names)
    fn get_all_names(&self, tree: &Tree, source: &str, node_type: &str) -> Vec<String>;
}

/// Trait for language-specific editing operations
pub trait LanguageEditor: Send + Sync {
    /// Apply a generic edit operation
    fn apply_operation(&self, operation: &crate::operations::EditOperation, source: &str) -> Result<crate::operations::EditResult>;
    
    /// Get detailed information about a node
    fn get_node_info(&self, tree: &Tree, source: &str, selector: &crate::operations::NodeSelector) -> Result<String>;
    
    /// Format code according to language conventions
    fn format_code(&self, source: &str) -> Result<String>;
    
    /// Validate that a replacement would create valid syntax
    fn validate_replacement(&self, original: &str, node: &Node, replacement: &str) -> Result<bool>;
}
