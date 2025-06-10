# Node Selection Redesign: Text-Anchored Selection

## Executive Summary

This document describes a fundamental redesign of the node selection system in semantic-edit-mcp, replacing the current multi-modal selector approach with a simple, universal text-anchored selection strategy.

**TL;DR**: Replace all current selector types with two parameters: `anchor_text` (exact string to find) and `ancestor_node_type` (walk up to this node type).

## Context and Motivation

### Problems with Current Approach

The existing node selection system uses four different selector types:

1. **Text Search**: Find all exact matches of `anchor_text` in source code
2. **Ancestor Walk**: For each match, attempt to walk up the AST to find `ancestor_node_type`
3. **Uniqueness Validation**: If successful walks ≠ 1, return error with details about all matches found
4. **Target Validation**: Verify the single resulting target node is suitable for editing2. **Type-based** (`NodeSelector::Type`) - Find first node of specified type  
3. **Position-based** (`NodeSelector::Position`) - Find nodes by line/column coordinates
4. **Query-based** (`NodeSelector::Query`) - Use tree-sitter queries


### Key Insight: Combination Uniqueness

The uniqueness requirement applies to the **combination** of `anchor_text` and `ancestor_node_type`, not just the anchor text alone. This allows for more flexible targeting while maintaining safety.

**Example: Multiple anchors, single valid target**
```rust
fn process() {
    let count = 5;           // ← "count" + walk to "let_declaration" ✓
    items.iter().map(|x| {
        x.count              // ← "count" + walk to "let_declaration" ✗ (no such ancestor)
    })
}

// This selector works:
anchor_text: "count"
ancestor_node_type: "let_declaration"
// Result: Targets the let declaration, not the field access
```

**Example: Multiple anchors, multiple valid targets (ambiguous)**
```json
{
  "stats": {"count": 42},        // ← "count" + walk to "pair" = pair 1
  "items": [{"count": 5}]        // ← "count" + walk to "pair" = pair 2  
}

// This selector fails:
anchor_text: "count"
ancestor_node_type: "pair"
// Result: Error - 2 valid targets found
```
### Key Issues Discovered

1. **Tree-sitter query limitations**: Unlike CSS selectors, tree-sitter queries lack positional selectors like `:nth-child()`, making it difficult to target specific instances of anonymous nested elements.
2. **Position-based fragility**: Line/column coordinates become invalid after any edit above the target position, making them unsuitable for AI agents performing multiple sequential edits.
3. **Cross-language inconsistency**: Different selector types work differently across languages, creating interface complexity.
4. **Anonymous element targeting**: Common editing tasks like "edit the 3rd match arm in a long match expression" are difficult or impossible with current selectors.

### Real-World Pain Points

- **Rust**: Editing non-first match arms in long functions
- **JSON**: Targeting specific properties in nested objects  
- **Markdown**: Editing specific items in nested lists
- **General**: Any scenario requiring "nth anonymous element" selection

## Design Constraints

### Hard Requirements

1. **Cross-language universality**: Must work identically across Rust, JSON, Markdown, and future languages
2. **AI agent friendly**: Must remain stable across multiple sequential edits
3. **Unique targeting**: Must unambiguously identify exactly one target node
4. **Implementation simplicity**: Should reduce rather than increase system complexity

### Design Principles

1. **Content as anchor**: Use actual file content to establish reliable reference points
2. **Structural navigation**: Use AST structure for precise targeting
3. **Fail-fast validation**: Reject ambiguous or invalid selections immediately
4. **Uniform interface**: Same selection mechanism across all languages and tools

## Proposed Solution: Text-Anchored Selection

### Core Concept

Replace all current selector types with a two-parameter system:

### Ambiguous Final Targeting
```
Error: anchor_text "count" with ancestor_node_type "pair" matches 2 nodes
Locations: 
  - Line 3: "count": 42 (in stats object)
  - Line 6: "count": 5 (in items array)
Suggestion: Use more specific anchor text to distinguish between matches
```

### No Valid Targets
```
Error: anchor_text "count" appears 2 times but no instances have ancestor "let_declaration"
Found ancestor types: pair, field_expression
Suggestion: Try ancestor_node_type "pair" instead
```
### Algorithm

1. **Text Search**: Find all exact matches of `anchor_text` in source code
2. **Uniqueness Validation**: If matches ≠ 1, return error with match count and locations
3. **Position Mapping**: Convert text position to AST node position
4. **Ancestor Walk**: Traverse up the AST until finding `ancestor_node_type`
5. **Target Validation**: Verify the target node exists and is suitable for editing

### Cross-Language Examples

**Rust - Edit specific match arm:**
```rust
// Target: Edit the POST handler in this match
match req.method {
    Method::GET => handle_get(),
    Method::POST => handle_post(),  // ← Target this arm
    Method::PUT => handle_put(),
}

// Selector:
anchor_text: "Method::POST =>"
ancestor_node_type: "match_arm"
```

**JSON - Edit specific property:**
```json
{
  "users": {
    "john": {"role": "admin"},
    "jane": {"role": "user"},  // ← Target this property
    "bob": {"role": "guest"}
  }
}

// Selector:
anchor_text: "\"jane\""
ancestor_node_type: "pair"
```

**Markdown - Edit specific list item:**
```markdown
# Features
- Authentication
  - Login
  - Registration  // ← Target this item
  - Password reset
- API endpoints

// Selector:
anchor_text: "Registration"
ancestor_node_type: "list_item"
```

## Advantages Over Current System

### 1. Universality
- Same interface works across all languages
- No language-specific selector logic needed
- Tree-sitter ancestor relationships are universal

### 2. Stability
- Text content is more stable than structural positions
- Edits that don't affect the anchor text don't break the selector
- No coordinate math required for AI agents

### 3. Predictability  
- Clear error messages when text is not unique
- Obvious anchor points visible in source code
- Deterministic target resolution

### 4. Simplicity
- Two parameters instead of complex selector variants
- Single implementation path instead of four different mechanisms
- Easier to test and validate

## Implementation Strategy

### Phase 1: Add New Selector Type
- Implement `TextAnchoredSelector` alongside existing selectors
- Add text search and ancestor walk functionality
- Create comprehensive test suite

### Phase 2: Tool Migration
- Update tools one by one to support new selector type
- Validate approach with real-world editing scenarios
- Gather feedback on ergonomics and edge cases

### Phase 3: Deprecation
- Mark old selector types as deprecated
- Update documentation and examples
- Eventually remove old selector implementations

## Error Handling and Edge Cases

### Ambiguous Text Matches
note: a human edited this and the examples are not realistic nesting of ancestors
```
Error: anchor_text "Method::POST" appears 3 times in file with matching ancestor `match_arm`:

1. Available ancestors: call_expression, block, function_item
  <first match with a few lines of context before and after>

2. Available ancestors: match_arm, match_block, function_item
  <second match with a few lines of context before and after>

3. Available ancestors: block, function_item
  <third match with a few lines of context before and after>
```

### No ancestor match for a single anchor match
```
Error: No ancestor of type "match_arm" found above anchor "Method::POST"
Available ancestors: call_expression, block, function_item
Suggestion: Try ancestor_node_type "call_expression" instead
```

### Multiple anchor matches but none of them have the chosen ancestor

note: a human edited this and the examples are not realistic nesting of ancestors

```
Error: anchor_text "Method::POST" appears 3 times in file but none of them match ancestor `match_arm`:


1. Available ancestors: call_expression, block, function_item
  <first match with a few lines of context before and after>

2. Available ancestors: if_statement, function_item
  <second match with a few lines of context before and after>

3. Available ancestors: block, function_item
  <third match with a few lines of context before and after>
```


### Invalid Anchor Text
```
Error: anchor_text "nonexistent" not found in file
Suggestion: Verify the exact text including whitespace and punctuation
```

## Limitations and Trade-offs

### Granularity Limitation
- Cannot target individual tokens/identifiers within expressions
- Minimum edit unit becomes the containing structural node
- **Mitigation**: Walk up to closest meaningful expression type

### Text Dependency
- Requires distinctive text near the target location  
- Breaks if anchor text is modified in same edit session
- **Mitigation**: Pathological cases are rare; fall back to parent node replacement

### Exact Match Requirement
- Case-sensitive and whitespace-sensitive matching
- No fuzzy matching or pattern matching
- **Rationale**: Precision over convenience for safety

## Future Extensions

### Potential Enhancements (if needed)
- Support for regex patterns in anchor text
- Multiple fallback ancestor types
- Relative positioning (nth-sibling, etc.)

### Integration Opportunities
- Auto-suggest distinctive anchor text for current cursor position
- Preview mode showing what would be selected before editing
- Integration with LSP for semantic anchor suggestions

## Migration Guide

### For Tool Implementers
```rust
// Old approach
NodeSelector::Name { 
    node_type: Some("function_item"), 
    name: "handle_request" 
}

// New approach  
NodeSelector {
    anchor_text: "fn handle_request",
    ancestor_node_type: "function_item"
}
```

### For AI Agents
```json
// Old tool call
{
  "name": "replace_node",
  "arguments": {
    "selector": {"type": "position", "line": 42, "column": 8},
    "new_content": "..."
  }
}

// New tool call
{
  "name": "replace_node", 
  "arguments": {
    "selector": {
      "anchor_text": "Method::POST =>",
      "ancestor_node_type": "match_arm"
    },
    "new_content": "..."
  }
}
```

## Open questions

- Is this sufficiently narrowing? Would it be helpful to add support for an optional outer named
  item, to be able to say "within the function named `main`, find the anchor text
  `SemanticEditServer::new` and walk up to the closest block," versus only being able to say "Find
  the anchor text `SemanticEditServer::new` and walk up to the closest block," which might be
  insufficiently narrowing in large files.

- Is the ancestor list discoverable for AI agents who haven't read this codebase? Do we need to
  include a translation layer from descriptive language? Is it sufficient to include schema
  documentation that encourages agents to omit the ancestor for discovery help?

## Success Metrics

### Reliability
- Zero coordinate invalidation issues for AI agents
- Reduced selector-related error rates
- Improved success rate for complex nested element targeting

### Usability  
- Consistent interface across all supported languages
- Clear, actionable error messages
- Reduced cognitive load for tool users

### Maintainability
- Single selector implementation path
- Language-agnostic selection logic
- Simplified testing and validation

## Conclusion

Text-anchored selection represents a fundamental simplification of the node selection problem. By leveraging content as a stable anchor point and AST structure for precise targeting, we can provide a universal, reliable, and maintainable selection mechanism that works consistently across all supported languages.

The approach trades some theoretical flexibility for significant practical benefits: stability across edits, cross-language consistency, and implementation simplicity. The limitations are acceptable given the rarity of pathological cases and the availability of parent node replacement as a fallback strategy.

This design aligns with the project's core values of safety, reliability, and AI-agent friendliness while significantly reducing system complexity.

---

*Document Status: Draft*  
*Created: December 2024*  
*Next Steps: Prototype implementation and validation*
