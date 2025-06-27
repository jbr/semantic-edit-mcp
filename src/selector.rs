use std::fmt::Display;

// Simplified text-based selector system
use anyhow::Result;
use schemars::JsonSchema;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Deserialize, Serialize, JsonSchema)]
#[serde(tag = "operation")]
pub enum Selector {
    #[serde(rename = "insert")]
    Insert {
        /// A unique exact snippet to position an insertion relative to
        ///
        /// This is whitespace sensitive
        anchor: String,

        /// Where in relation to that snippet to position the content
        position: InsertPosition,
    },
    #[serde(rename = "replace")]
    Replace {
        /// A complete exact text snippet to fully replace with the new content
        ///
        /// Important: This is mutually exclusive with the `from`/`to` pair.
        ///
        /// This is whitespace sensitive
        #[serde(skip_serializing_if = "Option::is_none")]
        exact: Option<String>,

        /// The beginning of a text range to replace with the new content
        ///
        /// Important: This is mutually exclusive with `exact`, and requires `to`
        ///
        /// Like other snippets, this is whitespace sensitive
        ///
        /// If the item you're replacing represents a syntactially-scoped region like a function or
        /// block, you may omit the `to` as long as the `from` snippet uniquely identifies the
        /// beginning of the block
        ///
        /// Examples: `fn my_function`, `def myfunc`, `if something_unique {`, `function something`
        #[serde(skip_serializing_if = "Option::is_none")]
        from: Option<String>,

        /// The end of a text range to replace with the new content
        ///
        /// Important: This is mutually exclusive with `exact`, and requires `from`.
        ///
        /// If your `from` does not describe a block, the first occurance of this unique string will
        /// describe the end of your replaced text.
        ///
        /// The outer edge will represent the end of the replaced content. This is whitespace
        /// sensitive.
        #[serde(skip_serializing_if = "Option::is_none")]
        to: Option<String>,
    },
}

// Claude Desktop doesn't know how to handle the fact that this is an anyOf and sends the selector
// as stringified json
pub fn deserialize_selector<'de, D>(deserializer: D) -> Result<Selector, D::Error>
where
    D: Deserializer<'de>,
{
    let value: Value = Deserialize::deserialize(deserializer)?;
    match value {
        Value::String(s) => serde_json::from_str(&s).map_err(serde::de::Error::custom),
        _ => Selector::deserialize(value).map_err(serde::de::Error::custom),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Copy)]
#[serde(rename_all = "snake_case")]
pub enum InsertPosition {
    #[serde(rename = "before")]
    Before,
    #[serde(rename = "after")]
    After,
}
impl InsertPosition {
    pub fn as_str(&self) -> &'static str {
        match self {
            InsertPosition::Before => "before",
            InsertPosition::After => "after",
        }
    }
}
impl Display for InsertPosition {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Selector {
    pub fn operation_name(&self) -> &str {
        match self {
            Selector::Insert { .. } => "Insert",
            Selector::Replace { .. } => "Replace",
        }
    }

    /// Validate that the selector is properly formed
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Selector::Insert { anchor, .. } => {
                if anchor.trim().is_empty() {
                    return Err("Insert anchor cannot be empty".to_string());
                }
                Ok(())
            }
            Selector::Replace { exact, from, to } => {
                match (exact.as_ref(), from.as_ref()) {
                    (Some(_), None) => {
                        // Exact replacement
                        if to.is_some() {
                            return Err(
                                "Cannot specify 'to' when using 'exact' replacement".to_string()
                            );
                        }
                        Ok(())
                    }
                    (None, Some(_)) => {
                        // Range replacement (to is optional)
                        Ok(())
                    }
                    (Some(_), Some(_)) => Err(
                        "Cannot specify both 'exact' and 'from' - use one or the other".to_string(),
                    ),
                    (None, None) => Err(
                        "Must specify either 'exact' or 'from' for replace operation".to_string(),
                    ),
                }
            }
        }
    }
}
