# New Features: AST Explorer Tool and Markdown Support

*Documentation for recent additions to the semantic-edit-mcp server*

## Overview

Two major features have been added to the semantic-edit-mcp server:

1. **AST Explorer Tool** - Interactive AST navigation and analysis
2. **Markdown Language Support** - Full editing capabilities for markdown files

These additions enhance the tool's usability and extend its multi-language capabilities.

---

## AST Explorer Tool

### What It Does

The `explore_ast` tool provides **interactive exploration of Abstract Syntax Trees** at any position in supported files. Think of it as a "developer X-ray vision" that shows you the internal structure of your code and helps you understand what nodes to target for editing operations.

### Key Features

#### 🎯 **Precise Node Analysis**
- Shows the exact AST node at any line/column position
- Provides multiple selector options for targeting the node
- Assesses edit suitability with intelligent recommendations

#### 📍 **Rich Context Information**
- **Ancestors**: Shows the complete hierarchy from the focus node to the root
- **Children**: Lists all child nodes of the focus node  
- **Siblings**: Shows neighboring nodes at the same level

#### 💡 **Intelligent Edit Recommendations**
- Language-specific suggestions for common operations
- Warns about problematic edit targets (like trying to edit just a `#` symbol)
- Suggests better alternatives when you select a poor target

#### 🎚️ **Multi-Language Support**
- **Rust**: Full analysis with semantic role identification
- **Markdown**: Heading, list, and section analysis
- **JSON**: Object and array structure analysis
- **Universal**: Syntax validation for any tree-sitter supported language

### Usage

```json
{
  "name": "explore_ast",
  "arguments": {
    "file_path": "src/lib.rs",
    "line": 42,
    "column": 15,
    "language": "rust"  // optional, auto-detected
  }
}
```

### Example Output

```
🔍 AST Exploration at 42:15

🎯 Focus: function_item (Function Definition)
   Content: "fn calculate_total(items: &[Item]) -> f64 { ... }"
   Position: lines 42-48, chars 1250-1456

📍 Context hierarchy (inner → outer):
  impl_item (Implementation Block)
    source_file (structural)

✅ Excellent edit target: Complete function - perfect for replacement or modification

💡 Edit Recommendations:
  1. Replace function by name (safer) - confidence: 98%
     Example: replace_node(file, {"name": "calculate_total"}, "fn new_function() { ... }")
  
  2. Replace entire function - confidence: 95%
     Example: replace_node(file, {"type": "function_item"}, "fn new_function() { ... }")
```

### When to Use the AST Explorer

#### ✅ **Perfect Use Cases**
- **Before making edits**: Understand exactly what you're targeting
- **Debugging failed operations**: See why a selector didn't work
- **Learning AST structure**: Understand how different languages parse
- **Finding better selectors**: Get recommendations for more reliable targeting

#### 🤔 **Example Workflow**
1. **Explore first**: `explore_ast(file, line, column)` to understand the structure
2. **Choose selector**: Pick from the recommended selectors
3. **Make edit**: Use the suggested operation with the recommended selector
4. **Validate**: Use preview mode to verify the change

### Language-Specific Features

#### **Rust Analysis**
```
🎯 Focus: function_item (Function Definition)
✅ Excellent edit target: Complete function - perfect for replacement

Available selectors:
  - By name: {"name": "calculate_total"} (confidence: 95%)
  - By type: {"type": "function_item"} (confidence: 70%)
  - By query: (function_item name: (identifier) @name (#eq? @name "calculate_total"))
```

#### **Markdown Analysis**
```
🎯 Focus: atx_heading (Heading Level 2)
✅ Excellent edit target: Complete heading

Available selectors:
  - By type: {"type": "atx_heading"} (confidence: 70%)
  - By position: {"line": 15, "column": 1} (confidence: 90%)

Edit Recommendations:
  - Replace heading text
  - Add content after heading
```

#### **JSON Analysis**
```
🎯 Focus: pair (Key-Value Pair)
✓ Good edit target: Key-value pair in JSON object

Edit Recommendations:
  - Add property to JSON object
  - Replace this key-value pair
```

### Edit Suitability Assessment

The tool provides intelligent feedback about whether a node is a good editing target:

#### ✅ **Excellent Targets**
- Complete functions, classes, structs
- Full headings in markdown
- Complete JSON objects/arrays

#### ✓ **Good Targets**  
- Smaller but complete constructs
- Individual list items
- Key-value pairs

#### ⚠️ **Poor Targets**
- Incomplete constructs
- Just identifiers without context
- Partial syntax elements

#### ❌ **Terrible Targets**
- Punctuation marks (`{`, `}`, `,`)
- Markdown markers (`#`, `-`, `*`)
- String quotes (`"`, `'`)

---

## Markdown Language Support

### What It Adds

Full markdown editing capabilities including:
- **Complete parsing** using tree-sitter-md
- **Syntax validation** for all edit operations
- **Semantic understanding** of markdown structure
- **Specialized editing recommendations**

### Supported Markdown Elements

#### **Headings**
```markdown
# Level 1 Heading
## Level 2 Heading  
### Level 3 Heading
```
- **AST nodes**: `atx_heading`, `atx_h1_marker`, `atx_h2_marker`, etc.
- **Editing**: Replace entire headings, modify heading text
- **Recommendations**: Add content after headings

#### **Lists**
```markdown
- Unordered item 1
- Unordered item 2

1. Ordered item 1
2. Ordered item 2
```
- **AST nodes**: `list`, `list_item`, `list_marker_minus`, etc.
- **Editing**: Replace lists, modify individual items, add new items
- **Recommendations**: Insert new list items

#### **Code Blocks**
````markdown
```rust
fn example() {
    println!("Hello, world!");
}
```
````
- **AST nodes**: `fenced_code_block`, `info_string`, `language`
- **Editing**: Replace entire code blocks, modify language tags
- **Language detection**: Extracts language from info string

#### **Block Quotes**
```markdown
> This is a block quote
> with multiple lines
```
- **AST nodes**: `block_quote`, `block_quote_marker`
- **Editing**: Modify quote content, replace entire quotes

#### **Sections**
Automatic grouping of content under headings
- **AST nodes**: `section`
- **Editing**: Add new sections, reorganize content

### Example Markdown Operations

#### **Replace a Heading**
```json
{
  "name": "replace_node",
  "arguments": {
    "file_path": "README.md",
    "selector": {"type": "atx_heading"},
    "new_content": "# Updated Project Title",
    "preview_only": true
  }
}
```

#### **Add Content After a Heading**  
```json
{
  "name": "insert_after_node",
  "arguments": {
    "file_path": "docs/guide.md",
    "selector": {"type": "atx_heading"},
    "content": "\n\nThis is new content under the heading.\n\n- Point 1\n- Point 2"
  }
}
```

#### **Add Item to a List**
```json
{
  "name": "insert_after_node", 
  "arguments": {
    "file_path": "TODO.md",
    "selector": {"type": "list"},
    "content": "\n- New task to complete"
  }
}
```

### AST Explorer with Markdown

The AST explorer provides particularly useful insights for markdown:

```
🎯 Focus: atx_heading (Heading Level 2)
   Content: "## Installation Guide"
   Position: lines 15-15, chars 200-220

📍 Context hierarchy:
  section (Document Section)
    document (structural)

✅ Excellent edit target: Complete heading - perfect for heading changes

💡 Edit Recommendations:
  1. Replace heading text (confidence: 95%)
     Example: replace_node(file, {"type": "atx_heading"}, "## New Title")
  
  2. Add content after heading (confidence: 90%)  
     Example: insert_after_node(file, {"type": "atx_heading"}, "\n\nNew content")
```

### Language Registry Update

Markdown is now registered in the language system:

```rust
// Automatically registered when server starts
languages.insert("markdown".to_string(), Box::new(MarkdownLanguage::new()?));
```

**File extensions supported**: `.md`, `.markdown`

---

## Integration with Existing Tools

### Enhanced Multi-Language Editing

All 16 existing tools now work with markdown:

#### **Core Tools** (now support markdown)
- `replace_node` - Replace any markdown element
- `insert_before_node` - Insert before headings, lists, etc.
- `insert_after_node` - Add content after sections, lists
- `wrap_node` - (Rust-specific, not applicable to markdown)

#### **Analysis Tools** (now support markdown)
- `validate_syntax` - Validate markdown syntax
- `get_node_info` - Inspect markdown AST nodes

### Tool Output Examples

#### **Successful Markdown Edit**
```
✅ Insert after operation result (syntax validation only):
Successfully inserted content after atx_heading node
```

#### **Blocked Invalid Markdown**
```
❌ Edit would create invalid syntax and was blocked:
  Line 15: Unclosed fenced code block
  Line 20: Invalid list marker sequence
```

---

## Architecture Impact

### Query-Based Language Support

Markdown uses the modern query-based architecture:

```rust
impl LanguageSupport for MarkdownLanguage {
    fn load_queries(&self) -> Result<LanguageQueries> {
        // Load from queries/markdown/operations.scm
        queries.operations = load_query_file(&language, "queries/markdown/operations.scm")?;
        Ok(queries)
    }
}
```

### File Structure
```
queries/markdown/
├── node-types.json     # Generated by tree-sitter-md
├── operations.scm      # Available edit operations
└── (additional query files as needed)
```

---

## Best Practices

### Using the AST Explorer Effectively

#### **1. Explore Before Editing**
```bash
# First, understand the structure
explore_ast(file, line, column)

# Then, make informed edits
replace_node(file, recommended_selector, new_content)
```

#### **2. Trust the Suitability Assessments**
- ✅ **Excellent/Good**: Proceed with confidence
- ⚠️ **Poor**: Consider the suggested alternatives
- ❌ **Terrible**: Always use a different node

#### **3. Use Name Selectors When Available**
```json
// Preferred (stable across edits)
{"name": "calculate_total"}

// Avoid (fragile to changes)  
{"line": 42, "column": 15}
```

### Markdown Editing Best Practices

#### **1. Target Complete Elements**
```markdown
# Good: Select the entire heading
## Project Overview

# Avoid: Just the ## markers
```

#### **2. Respect Markdown Structure**
- Add content after headings, not inside them
- Insert list items after existing lists
- Maintain proper nesting levels

#### **3. Use Semantic Selectors**
```json
// Good: Select by type
{"type": "atx_heading"}

// Good: Select by content for unique elements
{"content": "## Installation"}

// Avoid: Position-based for markdown (content shifts frequently)
{"line": 15, "column": 1}
```

---

## Migration Guide

### For Existing Users

No breaking changes! All existing tools continue to work exactly as before.

**New capabilities added:**
- `explore_ast` tool available immediately
- All editing tools now work with `.md` files
- Enhanced error messages for all languages

### For New Users

**Recommended workflow:**
1. **Start with exploration**: `explore_ast` to understand file structure
2. **Choose appropriate tools**: Use core tools for cross-language editing
3. **Leverage validation**: Always use preview mode for complex edits

---

## Performance and Limitations

### AST Explorer Performance
- **Fast for most files**: Sub-second response for files <10k lines
- **Rich analysis**: Comprehensive node information without slowdown
- **Memory efficient**: Doesn't cache trees, analyzes on-demand

### Markdown Support Performance
- **Full tree-sitter parsing**: Leverages fast, incremental parsing
- **Syntax validation**: Same two-layer validation as Rust/JSON
- **Large document handling**: Tested with documents up to 5k lines

### Current Limitations

#### **AST Explorer**
- No interactive navigation (single-shot exploration)
- Limited to one position per call
- Analysis quality varies by language sophistication

#### **Markdown Support**
- No context validation layer yet (only syntax validation)
- Limited smart editing features compared to Rust
- No markdown-specific specialized tools (like `insert_after_heading`)

---

## Future Enhancements

### AST Explorer Roadmap
- **Interactive mode**: Navigate AST with keyboard shortcuts
- **Visual tree display**: ASCII art tree representation
- **Batch analysis**: Explore multiple positions simultaneously
- **Pattern discovery**: Suggest common edit patterns

### Markdown Enhancement Plans
- **Context validation**: Semantic rules for markdown structure
- **Specialized tools**: `insert_after_heading`, `add_list_item`, etc.
- **Link analysis**: Understanding and editing markdown links
- **Table support**: Enhanced editing for markdown tables

### Integration Opportunities
- **Documentation generation**: Use markdown support for automated docs
- **Multi-language workflows**: Edit code and documentation together
- **Template systems**: Markdown templates with code generation

---

## Conclusion

The addition of the **AST Explorer tool** and **Markdown support** represents a significant enhancement to the semantic-edit-mcp server:

### **AST Explorer Benefits**
- 🔍 **Eliminates guesswork** when targeting nodes for editing
- 🧠 **Educational tool** for understanding AST structure
- 🎯 **Improves editing accuracy** through intelligent recommendations
- 🚀 **Speeds up development** by showing optimal selectors

### **Markdown Support Benefits**  
- 📝 **Documentation editing** capabilities 
- 🌍 **True multi-language workflow** (code + docs)
- ✅ **Same safety guarantees** as Rust and JSON
- 🔄 **Consistent tool interface** across all languages

**Combined Impact**: These features transform the tool from a code-focused editor into a comprehensive **multi-language semantic editing platform** with powerful introspection capabilities.

The self-development capability continues to work perfectly - we can now use the tool to improve its own documentation (this document was written knowing these tools exist!) and to add even more language support efficiently.

---

*Last Updated: December 8, 2024*  
*New Features Version: 0.1.3*  
*Total Tools: 17 (16 existing + 1 new AST explorer)*  
*Total Languages: 3 (Rust, JSON, Markdown)*