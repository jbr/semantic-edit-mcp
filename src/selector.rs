use crate::item::ItemRef;
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

    /// The text to target, copied from the file. Supply this **or** `item`.
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub anchor: Option<String>,

    /// Target a **named item** instead of anchor text: `{"kind": "function",
    /// "name": "handle_frame"}`. Supply this or `anchor`, not both.
    ///
    /// The item resolves together with its leading outer attributes and doc
    /// comments, so `insert_before` places new code above the whole decorated
    /// item and `insert_after` below it — the boundary a text anchor on the
    /// item's first line cannot express, and the one that silently gives a new
    /// function its neighbor's `#[test]` and documentation. The result names the
    /// item it resolved to and its neighbors, so the placement is checkable.
    ///
    /// Only for files with a grammar, and only for items that declare a name;
    /// anchor text remains the general path.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[arg(long, value_parser = str::parse::<ItemRef>)]
    pub item: Option<ItemRef>,
}

impl Selector {
    pub fn operation_name(&self) -> &str {
        self.operation.as_str()
    }

    /// The anchor text, when this selector targets text at all.
    pub fn anchor(&self) -> Option<&str> {
        self.anchor.as_deref()
    }

    /// Validate that the selector is properly formed
    pub fn validate(&self) -> Result<(), String> {
        let Self { anchor, item, .. } = self;

        let mut errors = vec![];
        match (anchor, item) {
            (Some(anchor), None) if anchor.trim().is_empty() => {
                errors.push("- `anchor` cannot be empty");
            }
            (None, None) => errors.push(
                "- supply either `anchor` (text copied from the file) or \
`item` ({kind, name})",
            ),
            (Some(_), Some(_)) => errors.push(
                "- supply `anchor` or `item`, not both: they name the target two \
different ways and could disagree",
            ),
            _ => {}
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
