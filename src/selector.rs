use std::fmt::Display;

// Simplified text-based selector system
use anyhow::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Copy)]
pub enum Operation {
    #[serde(rename = "insert_before")]
    InsertBefore,
    #[serde(rename = "insert_after")]
    InsertAfter,
    #[serde(rename = "insert_after_node")]
    InsertAfterNode,
    #[serde(rename = "replace_range")]
    ReplaceRange,
    #[serde(rename = "replace_exact")]
    ReplaceExact,
    #[serde(rename = "replace_node")]
    ReplaceNode,
}

impl Operation {
    pub fn as_str(&self) -> &'static str {
        match self {
            Operation::InsertBefore => "insert before",
            Operation::InsertAfter => "insert after",
            Operation::InsertAfterNode => "insert after node",
            Operation::ReplaceRange => "replace range",
            Operation::ReplaceExact => "replace exact",
            Operation::ReplaceNode => "replace node",
        }
    }
}
impl Display for Operation {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
pub struct Selector {
    /// The type of edit operation to perform.
    ///
    /// Insert Operations
    /// - **`insert_before`** - Insert content immediately before the anchor text
    /// - **`insert_after`** - Insert content immediately after the anchor text  
    /// - **`insert_after_node`** - Insert content after the complete AST node containing the anchor
    ///
    /// Replace Operations
    /// - **`replace_exact`** - Replace only the exact anchor text
    /// - **`replace_node`** - Replace the entire AST node containing the anchor
    /// - **`replace_range`** - Replace everything from anchor to end (requires `end` field)
    ///
    /// ## Choosing the Right Operation
    ///
    /// **Start with the simpler operations first - they usually work!**
    ///
    /// **For adding new code:**
    /// - **`insert_after_node`** - **Try this first** for adding methods, functions, or statements. Works great with simple anchors like `"methodName() {"`
    /// - Use `insert_before` or `insert_after` for precise placement within lines
    ///
    /// **For changing existing code:**
    /// - **`replace_node`** - **Try this first** for changing entire functions or methods. Use simple anchors like `"functionName() {"`
    /// - Use `replace_exact` for small, precise text changes
    /// - Use `replace_range` only when you need to replace across multiple different semantic boundaries
    ///
    /// **Common successful patterns:**
    /// - Adding a method to a class: `insert_after_node` with `"existingMethod() {"`
    /// - Replacing a function: `replace_node` with `"targetFunction() {"`
    /// - Only escalate to `replace_range` if the node-based operations don't work
    pub operation: Operation,

    /// Text to locate in the source code as the target for the operation.
    ///
    /// Should be a short, distinctive piece of text that uniquely identifies the location.
    /// For range operations, this marks the start of the range.
    /// For node operations, this should cover the start of the ast node.
    ///
    /// Tips for Good Anchors
    ///
    /// - **Start simple** - Try just the function signature first: `fn main() {`, `constructor() {`, `def method():`
    /// - **Function signatures work great** - No need for full method bodies or complex targeting
    /// - **Keep anchors short but unique** - "fn main" instead of the entire function signature
    /// - **Use distinctive text** - function names, keywords, or unique comments work well
    /// - **Avoid whitespace-only anchors** - they're often not unique enough
    /// - **Test your anchor** - if it appears multiple times, the tool will find the best placement
    /// - **When adding methods to classes** - `insert_after_node` with just the method signature usually works perfectly
    ///
    /// # Examples
    ///
    /// **Simple function/method signatures (recommended):**
    /// - `"fn main() {"` - Targets a Rust function definition
    /// - `"pub fn new() {"` - Targets a specific Rust method
    /// - `"constructor(apiKey) {"` - Targets a JavaScript constructor
    /// - `"async loadUser(id) {"` - Targets an async JavaScript method
    /// - `"def validate_email(email: str) -> bool:"` - Targets a Python function
    /// - `"clearCache(): void {"` - Targets a TypeScript method
    ///
    /// **Other useful patterns:**
    /// - `"struct User {"` - Targets a struct definition  
    /// - `"class UserManager {"` - Targets a class definition
    /// - `"// TODO: implement"` - Targets a specific comment
    /// - `"import React"` - Targets an import statement
    ///
    /// **For adding methods to classes, try `insert_after_node` first:**
    /// ```rust
    /// // Instead of complex targeting, just use:
    /// anchor: "existingMethod() {"
    /// operation: "insert_after_node"
    /// // This will add your new method right after the existing one
    /// ```
    ///
    /// **Troubleshooting:**
    /// - If your operation fails, try a simpler anchor first
    /// - "Function not found" → Check the exact function signature in the file
    /// - "Invalid syntax" → Usually means you need `insert_after_node` instead of `insert_after`
    /// - When in doubt: start with function signatures and `insert_after_node` or `replace_node`
    pub anchor: String,

    /// End boundary for replace range operations only.
    ///
    /// When specified, defines the end of the text range to be replaced.
    /// Use this to avoid repeating long blocks of content just to replace them.
    ///
    /// # Example
    /// ```json
    /// {
    ///   "operation": "replace_range",
    ///   "anchor": "// Start replacing here",
    ///   "end": "// Stop replacing here"
    /// }
    /// ```
    #[serde(skip_serializing_if = "Option::is_none")]
    pub end: Option<String>,
}

impl Selector {
    pub fn operation_name(&self) -> &str {
        self.operation.as_str()
    }

    /// Validate that the selector is properly formed
    pub fn validate(&self) -> Result<(), String> {
        let Self {
            operation,
            anchor,
            end,
        } = self;

        let mut errors = vec![];
        if anchor.trim().is_empty() {
            errors.push("- `anchor` cannot be empty");
        }

        match operation {
            Operation::InsertBefore | Operation::InsertAfter | Operation::InsertAfterNode => {
                if end.is_some() {
                    errors.push(
                        "- End is not relevant for insert operations. Did you mean to `replace`?",
                    );
                }
            }
            Operation::ReplaceRange => {
                if end.is_none() {
                    errors.push("- End is required for range replacement");
                }
            }
            Operation::ReplaceExact | Operation::ReplaceNode => {
                if end.is_some() {
                    errors.push("- `end` is not relevant for `replace_exact` operations. Did you intend to `replace_range`?");
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("\n"))
        }
    }
}
