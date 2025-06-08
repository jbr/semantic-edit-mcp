
## Phase 1.5: Better Preview Context 📋
**Goal**: Make targeting errors immediately obvious in previews

### Implementation:
- **Replace generic success messages** with contextual insertion point previews
- **Show 3-5 lines before target** with structural context  
- **Show clear insertion marker**: `// <-- INSERTION POINT: Content will be inserted here`
- **Show 3-5 lines after target** to reveal structural relationships
- **Show first few lines of content** (truncated if long)

### Example Output:
```rust
impl EditOperation {
    pub fn apply(&self, source_code: &str, language: &str) -> Result<EditResult> {
        // existing code...
    }
} // <-- INSERTION POINT: Content will be inserted here
  //     ^^^ Makes structural errors immediately obvious!

impl NodeSelector {
    // next impl...
```

### Benefits:
- ✅ **Instant recognition** of targeting errors
- ✅ **Structural context** preserved and highlighted  
- ✅ **Spatial awareness** - shows exactly where content goes
- ✅ **Catches punctuation targeting** and container boundary issues


# Enhanced Error Handling & Preview System Roadmap

## Current Status
We identified a critical usability issue: the tool warns about terrible edit targets but still allows destructive operations. This led to corrupted markdown when targeting list markers instead of list items.

## Phase 1: Enhanced Error Blocking with Auto-Exploration ⏳
**Goal**: Block terrible targets and automatically show better alternatives

### Currently Needed:
1. **Fix compilation errors in tools.rs** 
   - Added suitability check code in wrong scope (outside function)
   - Need to place validation inside `insert_after_node` function body
   - After the `let target_node = ...` assignment (around line 262)

2. **Make `analyze_node` public in ast_explorer.rs** ✅ (Done)

3. **Add validation to all core editing tools**:
   - `insert_after_node` ⏳ (In progress) 
   - `insert_before_node`
   - `replace_node` 
   - `wrap_node`

### Implementation Pattern:
```rust
// After finding target_node:
use crate::ast_explorer::{ASTExplorer, EditSuitability};
let node_info = ASTExplorer::analyze_node(&target_node, &source_code, &language);
if matches!(node_info.edit_suitability, EditSuitability::Terrible { .. }) {
    // Auto-run exploration and return helpful error with alternatives
    return Ok(format_terrible_target_error(&exploration));
}
```

## Phase 2: Markdown Semantic Context Validation 📋
**Goal**: Add redundant protection specifically for markdown editing

### Tasks:
1. **Add markdown-specific validation rules**:
   - Prevent editing list markers (`list_marker_minus`, `list_marker_plus`) 
   - Prevent editing heading markers (`atx_h1_marker`, etc.)
   - Prevent editing standalone punctuation

2. **Integrate with context validator**:
   - Extend `ContextValidator` to support markdown
   - Add to `validate_insertion()` logic

## Phase 3: Preview-First Operation Staging System 📋
**Goal**: Make all operations safe by default with preview + commit workflow

### Core Architecture:
```rust
pub struct EditSession {
    id: String,
    staged_operation: Option<StagedOperation>,
    file_timestamps: HashMap<PathBuf, SystemTime>,  // Change detection
}

pub struct StagedOperation {
    operation_type: String,        // "insert_after_node", etc.
    file_path: PathBuf,
    content: String,               // Retargetable content
    selector: NodeSelector,        // Retargetable selector  
    other_args: HashMap<String, Value>,
    preview_result: String,
}
```

### New Commands:
- `retarget(new_selector)` - Change targeting without repeating content
- `update_content(new_content)` - Refine content without changing target
- `commit()` - Apply staged operation
- Auto-discard previous operation when new one starts

### Key Design Decisions:
- **Mandatory preview**: All editing operations show preview first
- **Auto-discard**: New operations replace staged ones (no manual cancel needed)
- **No operation type changes**: Content is too different between insert/replace/wrap
- **No "show staged" command**: AI has perfect recall, humans can re-state
- **Timestamp-based change detection**: Simple file modification tracking

## Phase 4: Advanced Features 📋
**Goal**: Polish and optimize the experience

### Potential Enhancements:
- **Batch operations** (semantic selectors only - no line/column in batches)
- **Smart suggestions** in error messages with one-click fixes
- **Undo functionality** with session history
- **Markdown-specific semantic operations** (`add_list_item`, `insert_after_heading`)

## Success Metrics

### Phase 1 Success:
- ✅ Zero accidental edits of punctuation/markers
- ✅ Helpful auto-exploration on blocked edits
- ✅ Reduced tool calls for fixing targeting errors

### Phase 3 Success:  
- ✅ All operations are non-destructive by default
- ✅ Easy iteration on targeting and content
- ✅ Clear workflow: try → refine → commit

## Implementation Priority
1. **Fix current compilation errors** (immediate)
2. **Complete Phase 1** (this week)
3. **Phase 2 markdown validation** (nice-to-have)
4. **Phase 3 staging system** (major feature, plan carefully)

---

*Last Updated: December 7, 2024*
*Current Focus: Phase 1 - Enhanced Error Blocking*
