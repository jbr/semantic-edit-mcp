# Semantic Edit MCP

> [!CAUTION]
> This repository is written by AI as an experiment. Although I
> ([jbr](https://github.com/jbr)) review every line, the code quality is not necessarily identical
> to what I would have written. Caveat usor.

A Model Context Protocol (MCP) server for semantic code editing using tree-sitter. This server provides safe, AST-aware code editing operations that preserve syntax structure and formatting.

## Features

- **🔍 Semantic node targeting**: Find nodes by name, type, tree-sitter query, or position
- **🛡️ Safe structural editing**: Replace, insert, wrap, or delete AST nodes while maintaining syntax
- **✅ Syntax validation**: Validate code before and after edits to prevent breaking changes
- **👁️ Preview mode**: Test operations safely with `preview_only: true` - see changes without applying them
- **🦀 Rust support**: Currently supports Rust with extensible architecture for more languages
- **⚡ Transaction safety**: All edits are validated before being applied to files

## Installation

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

#### `replace_node`

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

#### `insert_before_node` / `insert_after_node`

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
  "preview_only": true
}
```

#### `validate_syntax`

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

#### `get_node_info`

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

## Preview Mode

**New in this release!** All editing operations support a `preview_only` parameter for safe exploration:

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

## Node Selectors

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

## Architecture

The project is organized into several modules:

- **`parsers/`**: Tree-sitter integration and language-specific parsing
- **`editors/`**: Language-specific editing logic (currently Rust)
- **`operations/`**: Core edit operations and node selection
- **`validation/`**: Syntax validation and error reporting
- **`schemas/`**: JSON schemas for tool parameters

## Safety Features

1. **👁️ Preview Mode**: Test operations with `preview_only: true` before applying
2. **✅ Syntax Validation**: All edits are validated before being applied
3. **🎯 AST-Aware Positioning**: Edits respect semantic boundaries
4. **⚡ Atomic Operations**: File changes are applied atomically
5. **📐 Format Preservation**: Maintains indentation and structure context

## Extending to New Languages

To add support for a new language:

1. Add the tree-sitter grammar dependency to `Cargo.toml`
2. Create a new parser module in `src/parsers/`
3. Create a new editor module in `src/editors/`
4. Update the language detection and dispatch logic

## Examples

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

### Add documentation to a struct (actually apply changes)

```json
{
  "name": "insert_before_node",
  "arguments": {
    "file_path": "src/lib.rs",
    "selector": {
      "type": "struct_item",
      "name": "MyStruct"
    },
    "content": "/// A well-documented struct\n/// \n/// This struct represents...",
    "preview_only": false
  }
}
```

### Test wrapping code in a conditional

```json
{
  "name": "wrap_node",
  "arguments": {
    "file_path": "src/main.rs",
    "selector": {
      "line": 25,
      "column": 4
    },
    "wrapper_template": "#[cfg(feature = \"advanced\")]\n{{content}}",
    "preview_only": true
  }
}
```

## License

MIT OR Apache-2.0
