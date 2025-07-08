use anyhow::Result;
use clap::ValueEnum;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::fmt::{self, Display, Formatter};
use strum::{EnumString, VariantNames};

#[derive(
    Debug,
    Clone,
    Deserialize,
    Serialize,
    JsonSchema,
    Copy,
    Eq,
    PartialEq,
    EnumString,
    VariantNames,
    ValueEnum,
)]
#[strum(serialize_all = "snake_case")]
pub enum Operation {
    #[serde(rename = "insert_after_node")]
    InsertAfterNode,
    #[serde(rename = "insert_before_node")]
    InsertBeforeNode,
    #[serde(rename = "replace_node")]
    ReplaceNode,
}

impl Operation {
    pub fn as_str(&self) -> &'static str {
        match self {
            Operation::InsertAfterNode => "insert after",
            Operation::InsertBeforeNode => "insert before",
            Operation::ReplaceNode => "replace",
        }
    }
}

impl Display for Operation {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema, Eq, PartialEq, clap::Args)]
pub struct Selector {
    /// The type of edit operation to perform.
    ///
    /// - **`insert_after_node`** - Insert content after the complete AST node containing the anchor
    /// - **`insert_before_node`** - Insert content before the complete AST node containing the anchor
    /// - **`replace_node`** - Replace the entire AST node that starts with the anchor text
    #[arg(value_enum)]
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
}

impl Selector {
    pub fn operation_name(&self) -> &str {
        self.operation.as_str()
    }

    /// Validate that the selector is properly formed
    pub fn validate(&self) -> Result<(), String> {
        let Self { anchor, .. } = self;

        let mut errors = vec![];
        if anchor.trim().is_empty() {
            errors.push("- `anchor` cannot be empty");
        }

        if anchor.contains('\n') {
            errors.push("- Multiline anchors are not supported. Use shorter, single-line anchors for better reliability.");
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("\n"))
        }
    }
}
