# Enhanced Snapshot Testing Framework Plan

## Current Gap Analysis

Our current snapshot testing framework has a significant gap: we're only testing **tool console output** but not the **actual file transformations** that the tools perform. This means we're missing critical aspects of tool behavior.

### What We Currently Test ✅
- Tool console output (error messages, preview text)
- Tool execution success/failure
- Error handling and suggestions

### What We're Missing ❌
- **Actual file transformations** - The real file changes that would be made
- **Preview vs actual consistency** - Whether preview matches actual changes
- **File corruption detection** - Whether transformations produce valid syntax
- **Attribute duplication bugs** - The specific issue we keep encountering

## Identified Problems from Current Testing

From our comprehensive test coverage with 13 test cases, we found:

1. **"Operation did not produce new content" issue** - Affects `replace_node` and `wrap_node`
2. **JSON query syntax error** - Blocks multi-language editing
3. **Duplicate attribute pattern** - User keeps hitting compilation errors due to duplicated `#[tokio::test]` attributes
4. **Preview reliability** - Some tools show good previews, others fail

## Enhanced Framework Design

### Proposed Directory Structure
```
tests/snapshots/basic_operations/replace_function/
├── input.rs              # Original file
├── args.json             # Tool arguments  
├── expected_output.txt   # Tool's console output (current)
└── expected_result.rs    # Expected transformed file (NEW!)
```

### Data Structure Changes Needed

```rust
#[derive(Debug)]
pub struct SnapshotTest {
    pub name: String,
    pub input_file: PathBuf,
    pub args_file: PathBuf,
    pub expected_output_file: PathBuf,
    pub expected_result_file: PathBuf,  // NEW: Expected file after transformation
}

#[derive(Debug)]
pub struct SnapshotResult {
    pub test: SnapshotTest,
    pub actual_output: String,
    pub expected_output: Option<String>,
    pub actual_result_file: Option<String>,     // NEW: Actual file content after transformation
    pub expected_result_file: Option<String>,   // NEW: Expected file content
    pub passed: bool,
    pub error: Option<String>,
}
```

### Enhanced Testing Workflow

#### Update Mode
1. **Execute tool with preview_only: false** (not just preview)
2. **Capture console output** → `expected_output.txt`
3. **Capture transformed file content** → `expected_result.rs`
4. **Write both to disk** for future comparison

#### Verify Mode
1. **Execute tool with preview_only: false**
2. **Compare console output** with `expected_output.txt`
3. **Compare transformed file** with `expected_result.rs`
4. **Validate syntax** of transformed file
5. **Report any mismatches**

### Implementation Tasks

#### Phase 1: Framework Enhancement
- [ ] Update `SnapshotTest` and `SnapshotResult` structures
- [ ] Modify test discovery to look for `expected_result.*` files
- [ ] Update `execute_test()` to run tools with `preview_only: false`
- [ ] Capture actual file transformations alongside console output
- [ ] Update comparison logic to check both outputs

#### Phase 2: Test Case Enhancement
- [ ] Regenerate all existing test cases with both outputs
- [ ] Add specific test cases for duplicate attribute scenarios
- [ ] Create test cases that validate syntax of transformed files
- [ ] Add test cases for preview vs actual consistency

#### Phase 3: Validation Features
- [ ] Add syntax validation for transformed files
- [ ] Add preview/actual consistency checking
- [ ] Add specific checks for common corruption patterns (duplicate attributes, etc.)

### Benefits of Enhanced Framework

1. **Catch File Corruption Early** - Would have caught duplicate attribute issues before they became user problems
2. **Preview/Actual Consistency** - Ensure previews accurately reflect what will actually happen
3. **Syntax Validation** - Automatically detect when transformations produce invalid code
4. **Regression Detection** - Changes to transformation logic will be immediately visible
5. **User Experience Quality** - Comprehensive testing of the actual user workflow

### Specific Issues This Would Solve

1. **Duplicate Attribute Pattern** - Test would show when replacement operations incorrectly preserve and duplicate existing attributes
2. **"Operation did not produce new content"** - Would show whether this is a preview-only issue or affects actual transformations too
3. **JSON Query Errors** - Would validate that multi-language operations actually work end-to-end
4. **Invisible File Corruption** - Would catch cases where tools silently corrupt files

## Implementation Priority

This enhancement should be prioritized because:

1. **Foundation for Quality** - Essential for reliable tool development
2. **User Problem Prevention** - Would prevent the frustrating duplicate attribute pattern
3. **Development Velocity** - Better testing leads to faster, more confident development
4. **Multi-Language Support** - Critical for validating JSON and other language support

## Next Session Action Items

1. **Implement enhanced snapshot framework** - Update data structures and testing logic
2. **Regenerate test cases** - Create expected_result files for all existing tests
3. **Investigate core issues** - Use enhanced testing to understand "Operation did not produce new content" problem
4. **Validate multi-language support** - Fix JSON operation issues with better testing

---

*Written: December 9, 2024*  
*Current Status: Framework gap identified, enhancement plan ready*  
*Next: Implement enhanced framework for comprehensive file transformation testing*
