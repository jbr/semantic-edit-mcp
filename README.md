# Semantic Edit MCP

> [!CAUTION]
> This repository is written by AI as an experiment. Although I
> ([jbr](https://github.com/jbr)) review every line, the code quality is not necessarily identical
> to what I would have written. Caveat usor.

> [!IMPORTANT]
> **FOR AI ASSISTANTS**: You cannot test changes to this MCP server without asking the user to restart Claude Desktop first. The MCP server must be recompiled and reloaded to test any code changes. Always pause and ask the user to restart Claude Desktop before attempting to test modifications.

A Model Context Protocol (MCP) server for semantic code editing using tree-sitter. This server provides safe, AST-aware code editing operations that preserve syntax structure and formatting.

## ✨ Features

- **🔍 Semantic node targeting**: Find nodes by name, type, tree-sitter query, or position
- **🛡️ Safe structural editing**: Replace, insert, wrap, or delete AST nodes while maintaining syntax
- **✅ Syntax validation**: Validate code before and after edits to prevent breaking changes
- **👁️ Preview mode**: Test operations safely with `preview_only: true` - see changes without applying them
- **🎯 Specialized insertion tools**: Smart, safe insertion at structural boundaries
- **💡 Enhanced error messages**: Intelligent suggestions and fuzzy matching for targeting mistakes
- **🦀 Rust support**: Currently supports Rust with extensible architecture for more languages
- **⚡ Transaction safety**: All edits are validated before being applied to files

## Installation

This project only builds on nightly rust because we use [let chains](https://github.com/rust-lang/rust/issues/53667)

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

### Available Tools

> **Note**: All tools currently support **Rust files only** (.rs files). Other languages will be added in future releases.

All editing tools support a `preview_only` parameter for safe testing:

#### Core Editing Tools

##### `replace_node`

Replace an entire AST node with new content.

```json
{
  "file_path": "src/main.rs",
  "selector": {
    "type": "function_item",
    "name": "main"
  },
  "new_content": "fn main() {\n    println!(\"Hello, semantic editing!\");\n}",
  "preview_only": true
}
```

##### `insert_before_node` / `insert_after_node`

Insert content before or after a specified node.

```json
{
  "file_path": "src/lib.rs",
  "selector": {
    "type": "function_item",
    "name": "existing_function"
  },
  "content": "/// New documentation\n#[derive(Debug)]",
  "preview_only": false
}
```

##### `wrap_node`

Wrap an existing node with new syntax.

```json
{
  "file_path": "src/lib.rs",
  "selector": {
    "line": 42,
    "column": 10
  },
  "wrapper_template": "if some_condition {\n    {{content}}\n}",
  "preview_only": true
}
```

#### ✨ Specialized Insertion Tools (New!)

These tools provide safer, more semantic insertion at structural boundaries:

##### `insert_after_struct`

Insert content after a struct definition.

```json
{
  "file_path": "src/lib.rs",
  "struct_name": "MyStruct",
  "content": "impl Default for MyStruct {\n    fn default() -> Self {\n        Self::new()\n    }\n}",
  "preview_only": true
}
```

##### `insert_after_enum`

Insert content after an enum definition.

```json
{
  "file_path": "src/lib.rs", 
  "enum_name": "Color",
  "content": "impl Color {\n    fn is_primary(&self) -> bool {\n        matches!(self, Color::Red | Color::Blue | Color::Yellow)\n    }\n}",
  "preview_only": false
}
```

##### `insert_after_impl`

Insert content after an impl block.

```json
{
  "file_path": "src/lib.rs",
  "impl_type": "MyStruct",
  "content": "impl Display for MyStruct {\n    fn fmt(&self, f: &mut Formatter) -> fmt::Result {\n        write!(f, \"MyStruct\")\n    }\n}",
  "preview_only": true
}
```

##### `insert_after_function`

Insert content after a function definition.

```json
{
  "file_path": "src/lib.rs",
  "function_name": "main",
  "content": "fn helper_function() -> i32 {\n    42\n}",
  "preview_only": false
}
```

##### `insert_in_module`

Smart module-level insertion with positioning control.

```json
{
  "file_path": "src/lib.rs",
  "content": "use std::collections::HashMap;",
  "position": "start",
  "preview_only": true
}
```

```json
{
  "file_path": "src/lib.rs", 
  "content": "#[cfg(test)]\nmod tests {\n    use super::*;\n}",
  "position": "end",
  "preview_only": false
}
```

#### Utility Tools

##### `validate_syntax`

Validate code syntax.

```json
{
  "file_path": "src/main.rs"
}
```

Or validate content directly:

```json
{
  "content": "fn test() { println!(\"test\"); }",
  "language": "rust"
}
```

##### `get_node_info`

Get information about a node at a specific location.

```json
{
  "file_path": "src/main.rs",
  "selector": {
    "line": 10,
    "column": 5
  }
}
```

## 👁️ Preview Mode

**Safe Testing**: All editing operations support a `preview_only` parameter:

- **`preview_only: true`**: Shows what would happen without modifying files, output prefixed with "PREVIEW:"
- **`preview_only: false`** (default): Actually applies the changes to files

This is perfect for:
- Testing complex operations safely
- Exploring AST structure and targeting
- AI agents "thinking through" edits before applying them

```json
{
  "name": "replace_node",
  "arguments": {
    "file_path": "src/main.rs",
    "selector": {"name": "main"},
    "new_content": "fn main() { println!(\"Testing!\"); }",
    "preview_only": true
  }
}
```

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

Node selectors allow you to target specific AST nodes using different strategies:

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

The project is organized into focused modules:

- **`server/`**: MCP protocol handling and server implementation
- **`tools/`**: Tool registry and core implementations
- **`specialized_tools/`**: Specialized insertion tools
- **`parsers/`**: Tree-sitter integration and language-specific parsing
- **`editors/`**: Language-specific editing logic (currently Rust)
- **`operations/`**: Core edit operations and node selection
- **`validation/`**: Syntax validation and error reporting
- **`handlers/`**: Request handling logic
- **`schemas/`**: JSON schemas for tool parameters

## 🛡️ Safety Features

1. **👁️ Preview Mode**: Test operations with `preview_only: true` before applying
2. **✅ Syntax Validation**: All edits are validated before being applied
3. **🎯 AST-Aware Positioning**: Edits respect semantic boundaries
4. **⚡ Atomic Operations**: File changes are applied atomically
5. **📐 Format Preservation**: Maintains indentation and structure context
6. **💡 Smart Error Messages**: Intelligent suggestions help avoid mistakes
7. **🔒 Specialized Tools**: Safe insertion at structural boundaries

## 🚀 Recent Improvements

### Phase 1 Features (Completed)
- ✅ **Preview Mode**: Safe testing for all operations
- ✅ **Enhanced Error Messages**: Fuzzy matching and intelligent suggestions
- ✅ **Specialized Insertion Tools**: 5 new tools for safer editing
- ✅ **Architecture Refactoring**: Modular, maintainable codebase
- ✅ **Extended Rust Support**: Comprehensive enum, impl, and module support

## 🔮 Extending to New Languages

To add support for a new language:

1. Add the tree-sitter grammar dependency to `Cargo.toml`
2. Create a new parser module in `src/parsers/`
3. Create a new editor module in `src/editors/`
4. Update the language detection and dispatch logic

## 📚 Examples

### Preview a function replacement (safe testing)

```json
{
  "name": "replace_node",
  "arguments": {
    "file_path": "src/main.rs",
    "selector": {
      "type": "function_item",
      "name": "risky_operation"
    },
    "new_content": "fn risky_operation() -> Result<(), Box<dyn Error>> {\n    // Safe implementation\n    Ok(())\n}",
    "preview_only": true
  }
}
```

### Add a trait implementation after a struct

```json
{
  "name": "insert_after_struct",
  "arguments": {
    "file_path": "src/lib.rs",
    "struct_name": "Point",
    "content": "impl Display for Point {\n    fn fmt(&self, f: &mut Formatter) -> fmt::Result {\n        write!(f, \"({}, {})\", self.x, self.y)\n    }\n}",
    "preview_only": false
  }
}
```

### Add tests at module level

```json
{
  "name": "insert_in_module",
  "arguments": {
    "file_path": "src/lib.rs",
    "content": "#[cfg(test)]\nmod tests {\n    use super::*;\n\n    #[test]\n    fn test_basic_functionality() {\n        assert!(true);\n    }\n}",
    "position": "end",
    "preview_only": true
  }
}
```

## License

MIT OR Apache-2.0
