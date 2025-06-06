# Semantic Edit MCP

> [!CAUTION]
> This repository is written by AI as an experiment. Although I
> ([jbr](https://github.com/jbr)) review every line, the code quality is not necessarily identical
> to what I would have written. Caveat usor.

A Model Context Protocol (MCP) server for semantic code editing using tree-sitter. This server provides safe, AST-aware code editing operations that preserve syntax structure and formatting.

## Features

- **Semantic node targeting**: Find nodes by name, type, tree-sitter query, or position
- **Safe structural editing**: Replace, insert, wrap, or delete AST nodes while maintaining syntax
- **Syntax validation**: Validate code before and after edits to prevent breaking changes
- **Multiple languages**: Currently supports Rust with extensible architecture for more languages
- **Transaction safety**: All edits are validated before being applied to files

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

#### `replace_node`

Replace an entire AST node with new content.

```json
{
  "file_path": "src/main.rs",
  "selector": {
    "type": "function_item",
    "name": "main"
  },
  "new_content": "fn main() {\n    println!(\"Hello, semantic editing!\");\n}"
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
  "content": "/// New documentation\n#[derive(Debug)]"
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
  "wrapper_template": "if some_condition {\n    {{content}}\n}"
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

## Node Selectors

Node selectors allow you to target specific AST nodes using different strategies:

### By Position
```json
{
  "line": 42,
  "column": 10
}
```

### By Name and Type
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

### By Tree-sitter Query
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

## Safety Features

1. **Syntax Validation**: All edits are validated before being applied
2. **AST-Aware Positioning**: Edits respect semantic boundaries
3. **Atomic Operations**: File changes are applied atomically
4. **Format Preservation**: Maintains indentation and structure context

## Extending to New Languages

To add support for a new language:

1. Add the tree-sitter grammar dependency to `Cargo.toml`
2. Create a new parser module in `src/parsers/`
3. Create a new editor module in `src/editors/`
4. Update the language detection and dispatch logic

## Examples

### Replace a function with error handling

```json
{
  "name": "replace_node",
  "arguments": {
    "file_path": "src/main.rs",
    "selector": {
      "type": "function_item",
      "name": "risky_operation"
    },
    "new_content": "fn risky_operation() -> Result<(), Box<dyn Error>> {\n    // Safe implementation\n    Ok(())\n}"
  }
}
```

### Add documentation to a struct

```json
{
  "name": "insert_before_node",
  "arguments": {
    "file_path": "src/lib.rs",
    "selector": {
      "type": "struct_item",
      "name": "MyStruct"
    },
    "content": "/// A well-documented struct\n/// \n/// This struct represents..."
  }
}
```

### Wrap code in a conditional

```json
{
  "name": "wrap_node",
  "arguments": {
    "file_path": "src/main.rs",
    "selector": {
      "line": 25,
      "column": 4
    },
    "wrapper_template": "#[cfg(feature = \"advanced\")]\n{{content}}"
  }
}
```

## License

MIT OR Apache-2.0
