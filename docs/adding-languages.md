# Adding New Language Support

This guide explains how to add support for a new programming language to the semantic editing MCP server.

## Overview

The semantic editing system has a multi-layered architecture for language support:

1. **Basic parsing** - Tree-sitter grammar for syntax understanding
## Current Language Support Status

- ✅ **Rust** - Full support (parsing, editing, context validation, syntax validation)
- ✅ **JSON** - Full support (parsing, editing, syntax validation)
- ✅ **Markdown** - Full support (parsing, editing, syntax validation)
- ⚠️ **Others** - Syntax validation only**Language-specific editing** - Custom logic for each language  
3. **Context validation** - Semantic rules to prevent invalid edits
4. **Syntax validation** - Tree-sitter based syntax checking (automatic)

## Current Language Support Status

- ✅ **Rust** - Full support (parsing, editing, context validation, syntax validation)
- ✅ **JSON** - Basic support (parsing, syntax validation)
- ✅ **TOML** - Basic support (parsing, syntax validation)
- ⚠️ **Others** - Syntax validation only

## Step-by-Step Guide

### Step 1: Add Tree-sitter Grammar Dependency

Add the tree-sitter grammar for your language to `Cargo.toml`:

```toml
[dependencies]
tree-sitter-python = "0.20"  # Example for Python
```

### Step 2: Create Language Support Module

Create a new file: `src/languages/{language}.rs`

```rust
### Step 4: Add to TreeSitterParser Registry

**CRITICAL**: Add your language to the parser registry in `src/parsers/mod.rs`:

```rust
impl TreeSitterParser {
    pub fn new() -> Result<Self> {
        let mut parsers = HashMap::new();

        // Existing parsers...
        let mut rust_parser = Parser::new();
        rust_parser.set_language(&tree_sitter_rust::LANGUAGE.into())?;
        parsers.insert("rust".to_string(), rust_parser);

        // Add your new language parser
        let mut python_parser = Parser::new();
        python_parser.set_language(&tree_sitter_python::LANGUAGE.into())?;
        parsers.insert("python".to_string(), python_parser);

        Ok(Self { parsers })
    }
}
```

**Without this step, you'll get "Unsupported language" errors even if everything else is set up correctly.**

### Step 5: Update Parser Detection```

### Step 3: Register in Language Registry

Update `src/languages/mod.rs`:

```rust
mod python;  // Add this line

use python::PythonSupport;  // Add this line

impl LanguageRegistry {
    pub fn new() -> Result<Self> {
        let mut languages = HashMap::new();
        
        // Existing languages...
        languages.insert("rust".to_string(), Box::new(RustSupport) as Box<dyn LanguageSupport>);
        
        // Add your new language
        languages.insert("python".to_string(), Box::new(PythonSupport) as Box<dyn LanguageSupport>);
        
        Ok(Self { languages })
    }
}
```

### Step 4: Update Parser Detection

Update `src/parsers/mod.rs` to include your language in `detect_language_from_path()`:

```rust
### Step 6: Add Context Validation (Optional)```

### Step 5: Add Context Validation (Optional)

If you want semantic validation for your language, update `src/validation/context_validator.rs`:

```rust
### Step 7: Test Your Implementation```

### Step 6: Test Your Implementation

```bash
cargo build
cargo test
```

Test with a simple edit:
```bash
# Test syntax validation
./target/debug/semantic-edit-mcp validate_syntax --file test.py

# Test node info
./target/debug/semantic-edit-mcp get_node_info --file test.py --line 1 --column 1
```

## Language-Specific Considerations

### For Simple Languages (JSON, YAML, etc.)
- Usually only need Steps 1, 2 (minimal), 3, and 4
- Context validation often not needed
- Focus on syntax validation and basic parsing

### For Complex Languages (Rust, Python, TypeScript, etc.)
- Need full implementation including context validation
- Require custom editing logic in the LanguageEditor
- May need language-specific node type handling

### Tree-sitter Query Examples

Context validation queries use tree-sitter query syntax:

```scheme
;; Prevent functions inside class methods (Python example)
(class_definition
  body: (block
    (function_definition) @invalid.function.in.class.body))

;; Prevent import statements in wrong locations
(function_definition
  body: (block
    (import_statement) @invalid.import.in.function))
```

## Making This Process Easier

### Current Pain Points

1. **Manual registration** - Each language needs manual updates in multiple files
2. **Boilerplate code** - Lots of repetitive implementation
3. **Tree-sitter query complexity** - Writing validation rules requires deep tree-sitter knowledge
4. **Testing overhead** - Need to test each language separately

### Future Improvements

#### 1. Language Discovery System
```rust
// Instead of manual registration, auto-discover languages
#[language_support(name = "python", extensions = ["py", "pyw"])]
struct PythonSupport {
    // Derive basic functionality automatically
}
```

#### 2. Validation Rule Templates
```yaml
# python-validation.yml
rules:
  - name: "no-functions-in-classes"
    description: "Functions cannot be directly in class bodies"
    query: "(class_definition body: (block (function_definition) @invalid))"
    severity: "error"
    suggestion: "Use method definitions instead"
```

#### 3. CLI Code Generator
```bash
# Generate language support boilerplate
cargo run -- generate-language python --grammar tree-sitter-python
```

#### 4. Testing Framework
```rust
#[language_test("python")]
fn test_python_validation() {
    // Auto-generated tests for common patterns
}
```

#### 5. Language Detection Improvements
- Use file content analysis (shebangs, syntax patterns)
- Support for multi-language files
- Better extension mapping

#### 6. Validation Rule Sharing
- Common validation patterns across languages
- Language-agnostic rules (e.g., "no items inside expressions")
- Rule inheritance system

## Examples

See `src/languages/rust.rs` for a complete implementation example.

See `src/languages/json.rs` for a minimal implementation example.

## Troubleshooting

### Common Issues

1. **"Language not found" errors** - Check language registry registration
2. **Parse errors** - Verify tree-sitter grammar dependency
3. **Validation not working** - Ensure context validator includes your language
4. **File detection failing** - Check extension mapping in parser detection

### Debug Commands

```bash
# Check if language is registered
cargo run -- get_node_info --file test.py --line 1 --column 1

# Test syntax validation
cargo run -- validate_syntax --file test.py

# Test context validation (if implemented)
cargo run -- validate_edit_context --file test.py --content "def foo(): pass" --operation_type insert_after --line 1 --column 1
```

## Contributing

When adding a new language:

1. **Start minimal** - Basic parsing and syntax validation first
2. **Add examples** - Include test files in `examples/`
3. **Document specifics** - Any language-specific quirks or limitations
4. **Test thoroughly** - Various file types and edge cases
5. **Update this guide** - Add your language to the support status

The goal is to make each language addition incrementally improve the system for everyone!
