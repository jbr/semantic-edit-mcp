# Multi-Language Support Implementation Plan

> **Context**: This plan was created after researching tree-sitter documentation and analyzing the current semantic-edit-mcp architecture. The goal is to add support for markup languages (JSON, TOML, Markdown) and other programming languages while leveraging tree-sitter's existing abstractions instead of creating competing ones.

## 🔍 **Key Tree-sitter Abstractions We Must Use**

After researching tree-sitter documentation, these are the built-in abstractions we should leverage:

1. **Node Types**: Tree-sitter already defines node types that correspond to grammar rules (e.g., `function_item`, `struct_item`, `object`, `atx_heading`)
2. **Query Language**: Tree-sitter has a powerful pattern-matching query language for finding syntax patterns
3. **Field Names**: Node children can be accessed by name instead of position (e.g., `name:`, `body:`)
4. **`node-types.json`**: Generated metadata about all possible syntax nodes in a grammar
5. **Standard Query Files**: Convention of `highlights.scm`, `locals.scm`, `tags.scm`, etc. in `queries/` directories

## 🎯 **Why Markup Languages First**

Starting with JSON, TOML, and Markdown instead of TypeScript will force better abstractions because:

- **No traditional functions/classes**: Forces us to think beyond code-centric concepts
- **Different semantic units**: JSON objects/properties, TOML sections/keys, Markdown headers/blocks  
- **Different edit operations**: Add JSON properties, insert TOML sections, create Markdown headers
- **Different validation**: Schema validation, key uniqueness, document structure

## 🏗️ **Architecture Overview**

### **Current State**
- Rust-specific logic scattered throughout `parsers/`, `editors/`, `tools.rs`
- Hardcoded node type handling in operations
- Manual tree-sitter query construction

### **Target State**
- Query-based language support using tree-sitter's native abstractions
- Language metadata loaded from `node-types.json`
- Operations defined in `.scm` query files
- Dynamic tool registration based on language capabilities

## 📋 **Implementation Plan**

### **Phase 1: Create Query-Based Language Abstractions**

#### **1.1 Define Language Support Trait**
```rust
// src/languages/traits.rs
pub trait LanguageSupport {
    fn language_name() -> &'static str;
    fn file_extensions() -> &'static [&'static str];
    fn tree_sitter_language() -> tree_sitter::Language;
    
    // Use tree-sitter's node types directly from node-types.json
    fn get_node_types() -> Result<Vec<NodeTypeInfo>>;
    
    // Load tree-sitter query files
    fn load_queries() -> Result<LanguageQueries>;
    
    fn parser() -> Box<dyn LanguageParser>;
    fn editor() -> Box<dyn LanguageEditor>;
}

#[derive(Debug, Clone)]
pub struct NodeTypeInfo {
    pub node_type: String,        // from node-types.json: "function_item", "object"
    pub named: bool,              // from node-types.json
    pub fields: Vec<String>,      // from node-types.json: field names like "name", "body"
    pub supports_search_by_name: bool,  // derived: has "name" field?
    pub display_name: String,     // human-readable: "Function", "JSON Object"
}

pub struct LanguageQueries {
    pub highlights: Option<Query>,
    pub locals: Option<Query>, 
    pub tags: Option<Query>,
    pub operations: Option<Query>,    // NEW: for semantic editing operations
    pub custom_queries: HashMap<String, Query>,
}
```

#### **1.2 Create Query-Based Parser**
```rust
// src/languages/query_parser.rs
pub struct QueryBasedParser {
    language: Language,
    queries: LanguageQueries,
    node_types: Vec<NodeTypeInfo>,
}

impl LanguageParser for QueryBasedParser {
    fn find_by_name(&self, tree: &Tree, source: &str, node_type: &str, name: &str) -> Result<Option<Node>> {
        // Build tree-sitter query dynamically based on node type metadata
        let node_info = self.get_node_type_info(node_type)?;
        
        let query_text = if node_info.fields.contains(&"name".to_string()) {
            format!(r#"({node_type} name: (identifier) @name (#eq? @name "{name}")) @target"#)
        } else if node_info.fields.contains(&"key".to_string()) {
            format!(r#"({node_type} key: (string) @key (#eq? @key '"{name}"')) @target"#)
        } else {
            return Err(anyhow!("Node type {} doesn't support name-based search", node_type));
        };
        
        self.execute_query(&query_text, tree, source)
    }
}
```

### **Phase 2: Implement JSON Support**

#### **2.1 Add Tree-sitter JSON Dependencies**
```toml
# Cargo.toml
tree-sitter-json = "0.24"
```

#### **2.2 Create JSON Language Implementation**
```rust
// src/languages/json/mod.rs
pub struct JsonLanguage;

impl LanguageSupport for JsonLanguage {
    fn language_name() -> &'static str { "json" }
    fn file_extensions() -> &'static [&'static str] { &["json"] }
    fn tree_sitter_language() -> tree_sitter::Language { tree_sitter_json::LANGUAGE.into() }
    
    fn get_node_types() -> Result<Vec<NodeTypeInfo>> {
        // Load from tree-sitter-generated node-types.json
        let node_types_json = include_str!("../../queries/json/node-types.json");
        parse_node_types_json(node_types_json)
    }
}
```

#### **2.3 Create JSON Query Files**
```scheme
;; queries/json/operations.scm
;; Find JSON objects for property insertion
(object) @insertable_object

;; Find JSON properties by key name
(object 
  (pair 
    key: (string) @key 
    (#eq? @key "TARGET_KEY")) @property)

;; Find JSON arrays for item insertion
(array) @insertable_array

;; Find values for replacement
(pair 
  key: (string) @key 
  value: (_) @value
  (#eq? @key "TARGET_KEY"))
```

#### **2.4 JSON-Specific Tools**
- `insert_json_property` - Add key-value pair to object
- `insert_json_array_item` - Add item to array  
- `replace_json_value` - Update value while preserving structure
- `wrap_in_json_object` - Wrap content in object structure

### **Phase 3: Add TOML Support**

#### **3.1 TOML Language Implementation**
```toml
tree-sitter-toml = "0.20"
```

```scheme
;; queries/toml/operations.scm
;; Find TOML tables by name
(table 
  (bare_key) @table_name 
  (#eq? @table_name "TARGET_TABLE")) @table

;; Find key-value pairs
(pair 
  (bare_key) @key 
  (#eq? @key "TARGET_KEY")) @pair

;; Find array of tables
(array_table) @array_table
```

#### **3.2 TOML-Specific Tools**
- `insert_toml_section` - Add new configuration section
- `insert_toml_key` - Add configuration key-value pair
- `insert_array_of_tables` - Add repeated configuration blocks

### **Phase 4: Add Markdown Support**

#### **4.1 Markdown Language Implementation**
```toml
tree-sitter-markdown = "0.7"
```

```scheme
;; queries/markdown/operations.scm
;; Find headers by level and text
(atx_heading 
  (atx_h1_marker) 
  (heading_content) @content 
  (#match? @content "TARGET_TEXT")) @header

;; Find code blocks by language
(fenced_code_block
  (info_string) @lang
  (#eq? @lang "TARGET_LANG")) @code_block

;; Find list items
(list_item) @list_item

;; Find paragraph boundaries
(paragraph) @paragraph
```

#### **4.2 Markdown-Specific Tools**
- `insert_markdown_section` - Add new header with content
- `insert_code_block` - Add fenced code block with language
- `insert_list_item` - Add item to existing list
- `wrap_in_markdown_block` - Wrap content in blockquote, etc.

### **Phase 5: Refactor Rust to Use New System**

#### **5.1 Move Rust to Query-Based Approach**
- Convert `parsers/rust.rs` to use `QueryBasedParser`
- Create `queries/rust/operations.scm` with Rust-specific queries
- Load Rust metadata from `node-types.json`

#### **5.2 Validate Architecture**
- Ensure all existing Rust functionality still works
- Prove the system handles both code and markup languages
- Test that abstractions work across very different language paradigms

### **Phase 6: Dynamic Tool System**

#### **6.1 Tool Generation from Metadata**
```rust
// Auto-generate tool schemas from node-types.json and operation queries
pub struct ToolGenerator;

impl ToolGenerator {
    pub fn generate_tools_for_language(lang: &dyn LanguageSupport) -> Vec<Tool> {
        let node_types = lang.get_node_types()?;
        let mut tools = Vec::new();
        
        // Generate standard tools for each searchable node type
        for node_type in node_types.iter().filter(|nt| nt.supports_search_by_name) {
            tools.push(Tool {
                name: format!("insert_after_{}", node_type.node_type),
                description: format!("Insert content after {} definition", node_type.display_name),
                input_schema: generate_schema_for_insertion(&node_type),
            });
        }
        
        // Add language-specific tools from operation queries
        tools.extend(generate_custom_tools_from_queries(lang.load_queries()?));
        
        tools
    }
}
```

## 🗂️ **File Structure After Implementation**

```
src/
├── languages/
│   ├── mod.rs              # Language registry and trait definitions
│   ├── traits.rs           # Core language support traits
│   ├── query_parser.rs     # Generic query-based parser
│   ├── json/
│   │   ├── mod.rs          # JSON language implementation
│   │   └── editor.rs       # JSON-specific editing logic
│   ├── toml/
│   │   ├── mod.rs          # TOML language implementation  
│   │   └── editor.rs       # TOML-specific editing logic
│   ├── markdown/
│   │   ├── mod.rs          # Markdown language implementation
│   │   └── editor.rs       # Markdown-specific editing logic
│   └── rust/
│       ├── mod.rs          # Rust implementation (refactored)
│       ├── parser.rs       # Rust parser (moved from parsers/rust.rs)
│       └── editor.rs       # Rust editor (moved from editors/rust.rs)
├── parsers/
│   └── mod.rs              # Generic parser registry (simplified)
├── editors/
│   └── mod.rs              # Generic editor registry (simplified)  
└── tools/
    ├── mod.rs              # Tool registry and generation
    └── generator.rs        # Dynamic tool generation from metadata

queries/
├── json/
│   ├── node-types.json     # Generated by tree-sitter-json
│   ├── operations.scm      # JSON-specific operation queries
│   └── highlights.scm      # JSON syntax highlighting
├── toml/
│   ├── node-types.json     # Generated by tree-sitter-toml
│   ├── operations.scm      # TOML-specific operation queries
│   └── highlights.scm      # TOML syntax highlighting
├── markdown/
│   ├── node-types.json     # Generated by tree-sitter-markdown
│   ├── operations.scm      # Markdown-specific operation queries
│   └── highlights.scm      # Markdown syntax highlighting
└── rust/
    ├── node-types.json     # Generated by tree-sitter-rust
    ├── operations.scm      # Rust-specific operation queries (refactored)
    └── validation.scm      # Existing Rust validation queries
```

## 🧪 **Testing Strategy**

1. **Unit Tests**: Each language implementation with representative files
2. **Integration Tests**: Cross-language operations and tool generation
3. **Regression Tests**: Ensure existing Rust functionality unchanged
4. **Query Tests**: Validate that `.scm` files produce expected matches

## 🚀 **Success Criteria**

1. **No Code Duplication**: All languages use the same query-based infrastructure
2. **Tree-sitter Native**: Leverage tree-sitter's abstractions instead of creating competing ones
3. **Dynamic Tool Generation**: Tools auto-generated from language metadata
4. **Extensible**: Adding new languages requires only implementing traits and adding query files
5. **Backwards Compatible**: All existing Rust functionality continues to work

## 🔄 **Migration Path**

1. Implement new architecture alongside existing code
2. Add JSON support as proof of concept
3. Add TOML and Markdown to validate flexibility
4. Migrate Rust to new system
5. Remove old architecture once validation complete

## 💡 **Key Implementation Notes**

- **Use tree-sitter's query language extensively** - don't reinvent pattern matching
- **Load metadata from `node-types.json`** - don't hardcode node type information
- **Follow tree-sitter conventions** - `queries/LANG/` directory structure
- **Make operations query-driven** - store complex logic in `.scm` files
- **Generate tools dynamically** - from language metadata and query capabilities

## 🔗 **Dependencies to Add**

```toml
# Add to Cargo.toml
tree-sitter-json = "0.24"
tree-sitter-toml = "0.20" 
tree-sitter-markdown = "0.7"

# For dynamic tool generation
serde_json = "1.0"  # Already included
include_dir = "0.7"  # For embedding query files
```

This plan leverages tree-sitter's existing, well-designed abstractions instead of creating competing ones, ensuring consistency with the broader tree-sitter ecosystem while enabling support for diverse language types from code to markup.
