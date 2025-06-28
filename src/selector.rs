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
    pub operation: Operation,
    pub anchor: String,
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
