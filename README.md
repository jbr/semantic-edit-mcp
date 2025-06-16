# Semantic Edit MCP

> [!CAUTION]
> This repository is written by AI as an experiment. Although I
> ([jbr](https://github.com/jbr)) review every line, the code quality is not necessarily identical
> to what I would have written. Caveat usor.

> [!IMPORTANT]
> **FOR AI ASSISTANTS**: You cannot test changes to this MCP server without asking the user to restart Claude Desktop first. The MCP server must be recompiled and reloaded to test any code changes. Always pause and ask the user to restart Claude Desktop before attempting to test modifications.

A Model Context Protocol (MCP) server for semantic code editing using tree-sitter. This server provides safe, AST-aware code editing operations that preserve syntax structure and prevent file corruption through comprehensive validation.

## ✨ Features

- **🌍 Multi-language support**: Rust (full), JSON (full), more languages easily added
- **🛡️ Two-layer validation**: Context validation + syntax validation prevents file corruption
- **🔍 Semantic node targeting**: Find nodes by name, type, tree-sitter query, or position
- **👁️ Preview mode**: Test operations safely with `preview_only: true` - see changes without applying them
- **🎯 Specialized insertion tools**: Smart, safe insertion at structural boundaries (Rust)
- **💡 Enhanced error messages**: Intelligent suggestions and fuzzy matching for targeting mistakes
- **⚡ Transaction safety**: All edits are validated before being applied to files
- **🏗️ Extensible architecture**: Easy to add support for new programming languages

## Language Support Status

- **🟢 Rust** - Full support (parsing, editing, context validation, syntax validation)
- **🟢 JSON** - Full support (parsing, editing, syntax validation)
- **🟡 Other languages** - Syntax validation only (easy to extend, not yet implemented)

## Installation

This project requires nightly Rust because we use [let chains](https://github.com/rust-lang/rust/issues/53667).

```bash
cargo install semantic-edit-mcp
```

## Usage

### As an MCP Server

Start the server:

```bash
semantic-edit-mcp serve
```

The server communicates via JSON-RPC over stdin/stdout and provides the following tools:

## Available Tools (16 Total)

### Core Multi-Language Editing Tools (4 tools)

All editing tools support full validation and work across supported languages:

#### `replace_node`

Replace an entire AST node with new content.

```json
{
  "file_path": "src/main.rs",
  "selector": {
    "type": "function_item",
    "name": "main"
  },
  "content": "fn main() {\n    println!(\"Hello, semantic editing!\");\n}",
}
```

#### `insert_before_node` / `insert_after_node`

Insert content before or after a specified node.

```json
{
  "file_path": "config.json",
  "selector": {
    "line": 3,
    "column": 21
  },
  "content": ",\n  \"description\": \"Added field\"",
}
```

#### `wrap_node`

Wrap an existing node with new syntax.

```json
{
  "file_path": "src/lib.rs",
  "selector": {
    "line": 42,
    "column": 10
  },
  "wrapper_template": "if some_condition {\n    {{content}}\n}",
}
```

### Analysis & Validation Tools (2 tools)

#### `validate_syntax`

Multi-language syntax validation.

```json
{
  "file_path": "src/main.rs"
}
```

Or validate content directly:

```json
{
  "content": "{\"key\": \"value\"}",
  "language": "json"
}
```

#### `get_node_info`

Multi-language node inspection.

```json
{
  "file_path": "config.json",
  "selector": {
    "line": 2,
    "column": 5
  }
}
```

### Rust-Specific Safe Insertion Tools (5 tools)

These tools provide safer, more semantic insertion at structural boundaries for Rust files:

#### `insert_after_struct`

Insert content after a struct definition.

```json
{
  "file_path": "src/lib.rs",
  "struct_name": "MyStruct",
  "content": "impl Default for MyStruct {\n    fn default() -> Self {\n        Self::new()\n    }\n}",
}
```

#### `insert_after_enum`

Insert content after an enum definition.

```json
{
  "file_path": "src/lib.rs", 
  "enum_name": "Color",
  "content": "impl Color {\n    fn is_primary(&self) -> bool {\n        matches!(self, Color::Red | Color::Blue | Color::Yellow)\n    }\n}",
}
```

#### `insert_after_impl`

Insert content after an impl block.

```json
{
  "file_path": "src/lib.rs",
  "impl_type": "MyStruct",
  "content": "impl Display for MyStruct {\n    fn fmt(&self, f: &mut Formatter) -> fmt::Result {\n        write!(f, \"MyStruct\")\n    }\n}",
}
```

#### `insert_after_function`

Insert content after a function definition.

```json
{
  "file_path": "src/lib.rs",
  "function_name": "main",
  "content": "fn helper_function() -> i32 {\n    42\n}",
}
```

#### `insert_in_module`

Smart module-level insertion with positioning control.

```json
{
  "file_path": "src/lib.rs",
  "content": "use std::collections::HashMap;",
  "position": "start",
}
```

## 🛡️ Comprehensive Validation System

### Two-Layer Validation

1. **Context Validation** (language-specific semantic rules)
   - Prevents functions inside struct fields
   - Prevents types inside function bodies
   - Available for Rust, more languages planned

2. **Syntax Validation** (all languages)
   - Tree-sitter parsing validation
   - Prevents syntax errors before writing files
   - Works with any tree-sitter supported language

### Validation Output Examples

```
✅ Replace operation result (with context validation):
Successfully replaced function_item node

✅ Insert after operation result (syntax validation only):
Successfully inserted content after pair node

❌ Edit would create invalid syntax and was blocked:
  Line 3: Missing }
  Line 4: Syntax error
```

## 👁️ Preview Mode

**Safe Testing**: All editing operations support a `preview_only` parameter:

- **`preview_only: true`**: Shows what would happen without modifying files, output prefixed with "PREVIEW:"
- **`preview_only: false`** (default): Actually applies the changes to files

Perfect for testing complex operations safely before applying them.

## 💡 Enhanced Error Messages

Get intelligent error messages with suggestions when targeting fails:

**Before:**
```
Error: Target node not found
```

**Now:**
```
Function 'mian' not found.

Available options: function: main, function: add, function: multiply

Did you mean: main
```

Features:
- **Fuzzy matching**: Suggests corrections for typos ("mian" → "main", "Pointt" → "Point")
- **Available options**: Lists all available functions, structs, enums, etc.
- **Context-aware**: Different suggestions based on what you're looking for

## 🎯 Node Selectors

Multiple ways to target nodes for editing:

### By Position
```json
{
  "line": 42,
  "column": 10
}
```

### By Name and Type (Recommended)
```json
{
  "type": "function_item",
  "name": "my_function"
}
```

### By Type Only
```json
{
  "type": "struct_item"
}
```

### By Tree-sitter Query (Advanced)
```json
{
  "query": "(function_item name: (identifier) @name (#eq? @name \"main\")) @function"
}
```

## 🏗️ Architecture

Multi-language semantic editing with pluggable language support:

- **`languages/`**: Language-specific support (Rust, JSON, extensible)
- **`validation/`**: Context validation and syntax validation
- **`tools/`**: Core editing tools with full validation
- **`parsers/`**: Multi-language tree-sitter integration
- **`operations/`**: Core edit operations and node selection

## 🔮 Adding New Languages

Our architecture makes it easy to add new programming languages:

1. **Basic support** (syntax validation only): ~2 hours
2. **Full support** (with context validation): ~1 day
3. **See [docs/adding-languages.md](docs/adding-languages.md)** for complete guide

## 📚 Examples

### Multi-Language Editing

#### Rust Function Replacement
```json
{
  "name": "replace_node",
  "arguments": {
    "file_path": "src/main.rs",
    "selector": {"type": "function_item", "name": "main"},
    "new_content": "fn main() -> Result<(), Box<dyn Error>> {\n    println!(\"Safe main!\");\n    Ok(())\n}",
    }
}
```

#### JSON Property Addition
```json
{
  "name": "insert_after_node",
  "arguments": {
    "file_path": "package.json",
    "selector": {"line": 3, "column": 20},
    "content": ",\n  \"description\": \"Updated package\"",
    }
}
```

### Safe Rust-Specific Operations

#### Add trait implementation after struct
```json
{
  "name": "insert_after_struct",
  "arguments": {
    "file_path": "src/lib.rs",
    "struct_name": "Point",
    "content": "impl Display for Point {\n    fn fmt(&self, f: &mut Formatter) -> fmt::Result {\n        write!(f, \"({}, {})\", self.x, self.y)\n    }\n}",
    }
}
```

#### Add tests at module level
```json
{
  "name": "insert_in_module",
  "arguments": {
    "file_path": "src/lib.rs",
    "content": "#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn test_basic_functionality() {\n        assert!(true);\n    }\n}",
    "position": "end",
    }
}
```

## 🚀 Recent Achievements (December 2024)

### ✅ Multi-Language Architecture Complete
- Language-aware validation system
- JSON editing support
- Extensible language registry
- Syntax validation safety net for all languages

### ✅ Comprehensive Validation System  
- Two-layer validation prevents file corruption
- Context validation for supported languages
- Syntax validation for all languages
- Zero file corruption incidents since implementation

### ✅ Enhanced Developer Experience
- Preview mode for safe testing
- Intelligent error messages with fuzzy matching
- Specialized tools for common Rust patterns
- Consistent validation across all tools

## 🔮 Future Enhancements

### Next Language Targets
- **Markdown** - Documentation editing (in progress)
- **Python** - High demand, good tree-sitter support  
- **TypeScript** - JavaScript ecosystem support
- **YAML** - Configuration files

### Advanced Features
- Cross-language operations
- Project-aware validation  
- Batch editing with transactions
- IDE integration (VS Code extension)

## License

MIT OR Apache-2.0
