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
    #[serde(rename = "insert_after")]
    InsertAfter,
    #[serde(rename = "insert_before")]
    InsertBefore,
    #[serde(rename = "replace")]
    Replace,
}

impl Operation {
    pub fn as_str(&self) -> &'static str {
        match self {
            Operation::InsertAfter => "insert after",
            Operation::InsertBefore => "insert before",
            Operation::Replace => "replace",
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
    /// - **`replace`** - Replace the code matched by `anchor` with `content` (omit `content` to delete it)
    /// - **`insert_after`** - Insert `content` immediately after the code matched by `anchor`
    /// - **`insert_before`** - Insert `content` immediately before the code matched by `anchor`
    #[arg(value_enum)]
    pub operation: Operation,

    /// The text to target, copied from the file.
    ///
    /// Provide the complete text of the code you're operating on — for
    /// `replace`, the whole item or statement being replaced; for inserts, the
    /// whole item the new code goes next to (a complete function, field,
    /// entry, statement, …).
    ///
    /// Matching is whitespace-insensitive: differences in line breaks and
    /// indentation are ignored, so the anchor doesn't need to reproduce the
    /// file's formatting exactly.
    ///
    /// If the anchor matches in more than one place, the first match is edited
    /// and the response lists every match location — extend the anchor with
    /// more of the target's own text if the wrong one was chosen. If the
    /// anchor isn't found, or doesn't line up with complete syntax nodes,
    /// nothing is changed.
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

        // if anchor.contains('\n') {
        //     errors.push("- Multiline anchors are not supported. Use shorter, single-line anchors for better reliability.");
        // }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(errors.join("\n"))
        }
    }
}
