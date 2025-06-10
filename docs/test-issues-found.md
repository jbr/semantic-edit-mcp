# Test Issues Found - December 2024

## Critical Preview Mode Issues (Highest Priority)
- `basic_operations/replace_function` - "Operation did not produce new content" instead of preview
- `basic_operations/wrap_node` - "Operation did not produce new content" instead of preview  
- `bug_reproductions/attribute_replacement_safety` - "Operation did not produce new content" instead of preview

## Critical Language Support Issues (High Priority)
- `json_operations/replace_property` - Query syntax error breaks JSON editing entirely
- `markdown_operations/insert_table_row` - Valid table insertion blocked by overly strict validation

## Markdown Structural Issues (Medium Priority)
- `markdown_operations/replace_heading_level` - Produces malformed headings (## ###)
- `markdown_operations/insert_list_item` - Extra newlines break list continuity + missing newlines concatenate content
- `markdown_operations/insert_ordered_list_item` - Same newline issues as above
- `markdown_operations/delete_heading_double_newline` - Leaves extra newlines
- `markdown_operations/insert_code_block` - Missing newline after insertion
- `markdown_operations/insert_link_in_paragraph` - Missing newline causes content concatenation
- `markdown_operations/insert_nested_list_item` - Missing newline breaks nested list structure
- `markdown_operations/insert_paragraph_between_lists` - Missing newline before heading
- `markdown_operations/replace_code_block` - Missing newline after replacement
- `markdown_operations/wrap_text_with_emphasis` - Malformed wrapping with line breaks

## Root Causes Identified
1. **Preview mode broken** for replace and wrap operations
2. **JSON tree-sitter queries malformed** 
3. **Markdown lacks language-specific newline handling**
4. **Query targeting wrong nodes** (targeting `inline` instead of `atx_heading`)

## Next Steps
1. Fix preview mode for core operations
2. Fix JSON query generation 
3. Implement markdown-aware newline handling
4. Fix markdown query targeting
