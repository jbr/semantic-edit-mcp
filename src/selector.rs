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
    /// - **`insert_after_node`** - Insert content after the complete AST node containing the anchor
    /// - **`insert_before`** - Insert content immediately before the anchor text
    /// - **`insert_after`** - Insert content immediately after the anchor text  
    ///
    /// Replace Operations
    /// - **`replace_node`** - Replace the entire AST node that starts with the anchor text
    /// - **`replace_exact`** - Replace only the exact anchor text -- only for short replacements
    /// - **`replace_range`** - Replace everything from anchor to `end`, inclusive (requires `end` field)
    ///
    /// ## Choosing the Right Operation
    ///
    /// **For adding new code:**
    /// - Use `insert_before` or `insert_after` for precise placement
    /// - Use `insert_after_node` when you want to add after a complete statement/declaration
    ///
    /// **For changing existing code:**
    /// - Use `replace_exact` for small, precise text changes shorter than a line of code.
    /// - Use `replace_node` for changing entire functions, classes, blocks, or statements
    /// - Use `replace_range` for changing multi-line sections with clear start/end boundaries
    pub operation: Operation,

    /// Text to locate in the source code as the target for the operation.
    ///
    /// Should be a short, distinctive piece of text that uniquely identifies the location.
    /// For range operations, this marks the start of the range.
    /// For node operations, this should cover the start of the ast node.
    ///
    /// Tips for Good Anchors
    ///
    /// - **Keep anchors short but unique** - "fn main" instead of the entire function signature
    /// - **Use distinctive text** - function names, keywords, or unique comments work well
    /// - **Avoid whitespace-only anchors** - they're often not unique enough
    /// - **Test your anchor** - if it appears multiple times, the tool will attempt to find the best placement
    ///
    /// # Examples
    /// - `"fn main"` - Targets a function definition
    /// - `"struct User"` - Targets a struct definition  
    /// - `"// TODO: implement"` - Targets a specific comment
    /// - `"import React"` - Targets an import statement
    pub anchor: String,

    /// End boundary for replace range operations only.
    ///
    /// When specified, defines the end of the text range to be replaced, inclusively.
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
