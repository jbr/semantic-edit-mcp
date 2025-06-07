# Semantic Edit MCP - Improvement Roadmap

## Background

During development and testing, we identified several areas where the semantic editing tool could be enhanced to prevent syntax errors and improve user experience. This document outlines potential improvements based on real-world usage patterns.

## 🚨 **Lessons from the Enum Syntax Error**

During self-improvement of the tool, we encountered a syntax error when trying to add a helper function inside an enum definition. This highlighted the need for better safety mechanisms and recovery strategies.

### What Happened
- Attempted to insert a function after an enum field instead of after the entire enum
- Created malformed syntax that broke the AST structure
- Required complete file rewrite to recover

### Key Insights
1. **Order of operations matters** for complex structural changes
2. **Target selection is critical** for insertion operations
3. **Recovery mechanisms work** but could be more graceful
4. **The tool correctly refused to make bad edits** (safety by design)

## 🛡️ **Safety Improvements**

### **Phase 1: Quick Wins (✅ COMPLETED)**

#### 1. ✅ Dry-Run Mode - IMPLEMENTED
Added preview functionality to all operations:

```json
{
  "name": "replace_node", 
  "arguments": {
    "file_path": "src/main.rs",
    "selector": {"type": "function_item", "name": "main"},
    "new_content": "fn main() { println!(\"test\"); }",
    "preview_only": true  // Shows result without writing
  }
}
```

**Benefits:** ✅ **ACHIEVED**
- ✅ Zero-risk preview of changes - Files remain unchanged with `preview_only: true`
- ✅ Validate complex operations before applying - "PREVIEW:" prefix clearly indicates preview mode
- ✅ Better for AI agents to "think through" edits - Prevents syntax errors during development
- ✅ Clear visual feedback - Operations show "PREVIEW:" prefix when in preview mode

**Implementation:** ✅ **COMPLETED**
- ✅ Added `preview_only: Option<bool>` parameter to all edit operations
- ✅ Updated JSON schemas for all tools (replace_node, insert_before_node, insert_after_node, wrap_node)
- ✅ Implemented preview logic in EditOperation.apply() method in RustEditor
- ✅ Updated file writing conditions in main.rs tool methods
- ✅ Added "PREVIEW:" prefix to operation result messages
- ✅ **TESTED**: All operations work correctly in both preview and actual modes

#### 2. Enhanced Error Messages
Replace generic errors with actionable feedback:

**Current:** `"Target node not found"`

**Improved:** `"Function 'missing_func' not found. Available functions: main, parse_selector, handle_request. Did you mean 'parse_selector'?"`

**Implementation:** Enhance error messages in `NodeSelector::find_node()` with:
- List of available alternatives
- Fuzzy matching suggestions  
- Context about what was actually found at the location

#### 3. Specialized Insertion Tools
Reduce targeting mistakes with semantic insertion helpers:

```json
{
  "name": "insert_after_struct",
  "description": "Insert content after a struct definition (safe structural boundary)",
  "arguments": {
    "file_path": "src/main.rs", 
    "struct_name": "NodeSelector",
    "content": "fn helper() {}"
  }
}
```

**New Tools to Add:**
- `insert_after_struct` - After struct definitions
- `insert_after_enum` - After enum definitions  
- `insert_after_impl` - After impl blocks
- `insert_in_module` - At module level (top-level items)
- `insert_after_function` - After function definitions

### **Phase 2: Advanced Safety (Future)**

#### 1. Transaction-Based Editing
Atomic multi-operation edits:

```rust
pub struct EditTransaction {
    operations: Vec<EditOperation>,
    rollback_content: String,
}

impl EditTransaction {
    pub fn commit(&self, file_path: &str) -> Result<EditResult> {
        // Apply all operations, validate syntax, then write
        // Roll back entirely if any operation fails
    }
}
```

**Use Cases:**
- Multi-step refactoring operations
- Batch changes across multiple functions
- Complex structural modifications

#### 2. Better Context-Aware Insertion
Analyze insertion context automatically:

```rust
pub enum InsertionContext {
    AfterEnum,
    AfterImpl, 
    AfterFunction,
    InModule,
    BeforeItem,
}

fn infer_safe_insertion_point(target_node: &Node) -> InsertionContext
```

#### 3. Automatic Backup/Restore
- Auto-backup files before major changes
- Easy rollback to previous versions
- Integration with Git when available

### **Phase 3: AI-Specific Features (Long Term)**

#### 1. Batch Operation Validation
Analyze multiple operations for conflicts before applying any:

```json
{
  "name": "validate_operation_batch",
  "arguments": {
    "operations": [
      {"type": "replace_node", "target": "...", "content": "..."},
      {"type": "insert_after", "target": "...", "content": "..."}
    ]
  }
}
```

#### 2. Automatic Operation Reordering
Intelligently reorder operations to avoid conflicts:
- Structural changes before insertions
- Dependency-aware operation sequencing
- Conflict detection and resolution

#### 3. LLM-Guided Error Recovery
- Generate fix suggestions for common errors
- Automatic retry with corrected targeting
- Learning from previous error patterns

## 🤔 **Design Philosophy: Safety vs. Complexity**

### Arguments AGAINST Over-Engineering

1. **Rare Occurrence**: Syntax errors from targeting mistakes are uncommon with careful usage
2. **Learning Curve**: Complex transaction systems might confuse users
3. **Recovery Works**: The `write_file` fallback was effective and clean
4. **Tool Complexity**: More features = more potential bugs and maintenance burden
5. **Human Factor**: Better documentation and user education might be more effective

### Arguments FOR Safety Improvements

1. **AI Usage**: AI agents might make targeting mistakes more frequently than humans
2. **Batch Operations**: Multiple edits compound the risk of structural conflicts
3. **User Confidence**: Developers would trust the tool more with better safety nets
4. **Professional Use**: Production environments demand maximum safety and reliability
5. **Learning Tool**: Better error messages help users understand AST structure

## 🎯 **Recommended Implementation Strategy**

### **Priority 1: Quick Wins (NEXT RELEASE)**
- ✅ **Add dry-run mode** - COMPLETED
- 🔄 **Better error messages** - Easy to implement, significantly improves UX  
- 🔄 **Specialized insertion tools** - Reduces common targeting mistakes

### **Priority 2: Monitor and Decide (After Usage Data)**
- ⏸️ **Transaction system** - Complex, implement only if multi-operation use cases emerge
- ⏸️ **Auto-backup** - Useful but may overlap with existing Git workflows
- ⏸️ **Context inference** - Sophisticated but may not provide enough value

### **Priority 3: Future Research (Long Term)**
- 📊 **AI-specific features** - Wait for clear AI agent usage patterns
- 📊 **Batch validation** - Implement when batch operations become common
- 📊 **LLM integration** - Experimental, needs careful design

## 🚀 **Implementation Notes**

### For Dry-Run Mode (✅ COMPLETED)
```rust
// IMPLEMENTED: Added to EditOperation enum
#[derive(Debug, Clone)]
pub enum EditOperation {
    Replace {
        target: NodeSelector,
        new_content: String,
        preview_only: Option<bool>,  // ✅ Added
    },
    // ... other variants all have preview_only field
}

// IMPLEMENTED: Modified apply() method
impl EditOperation {
    pub fn is_preview_only(&self) -> bool {
        // Returns true if any operation has preview_only: true
    }
}
```

### For Enhanced Error Messages
```rust
// Enhance NodeSelector::find_node()
impl NodeSelector {
    pub fn find_node_with_suggestions<'a>(&self, tree: &'a Tree, source_code: &str, language: &str) 
        -> Result<Option<Node<'a>>, DetailedError> {
        
        match self.find_node(tree, source_code, language) {
            Ok(Some(node)) => Ok(Some(node)),
            Ok(None) => {
                let suggestions = self.generate_suggestions(tree, source_code, language);
                Err(DetailedError::NotFound { 
                    selector: self.clone(), 
                    suggestions 
                })
            },
            Err(e) => Err(DetailedError::Other(e)),
        }
    }
}
```

## 📊 **Success Metrics**

To determine if these improvements are valuable:

1. **Error Reduction**: Track syntax error frequency before/after improvements
2. **User Confidence**: Survey users about trust in the tool
3. **Recovery Time**: Measure time to fix errors when they occur
4. **Feature Usage**: Monitor which safety features are actually used
5. **AI Agent Performance**: Track success rates for automated usage

## 🔄 **Review and Update Process**

This roadmap should be revisited:
- **After 3 months** of usage data collection
- **When adding new languages** (different AST complexities)
- **Based on user feedback** and error reports
- **When AI usage patterns emerge** and stabilize

---

*Last Updated: June 6, 2025*  
*Status: Phase 1 Dry-Run Mode Completed*  
*Next Review: September 2025*
