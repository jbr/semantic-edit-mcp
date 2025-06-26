// Simplified text-based selector system
use anyhow::Result;
use ropey::Rope;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "operation")]
pub enum Selector {
    #[serde(rename = "insert")]
    Insert {
        /// A unique exact snippet to position an insertion relative to
        anchor: String,
        /// Where in relation to that snippet to position the content
        position: InsertPosition,
    },
    #[serde(rename = "replace")]
    Replace {
        /// A complete exact text snippet to fully replace with the new content
        /// Important: This is mutually exclusive with the `from`/`to` pair.
        #[serde(skip_serializing_if = "Option::is_none")]
        exact: Option<String>,

        /// The beginning of a text range to replace with the new content
        /// Important: This is mutually exclusive with `exact`, and requires `to`
        #[serde(skip_serializing_if = "Option::is_none")]
        from: Option<String>,

        /// The end of a text range to replace with the new content
        /// Important: This is mutually exclusive with `exact`, and requires `from`
        #[serde(skip_serializing_if = "Option::is_none")]
        to: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema, Copy)]
#[serde(rename_all = "snake_case")]
pub enum InsertPosition {
    #[serde(rename = "before")]
    Before,
    #[serde(rename = "after")]
    After,
}

impl Selector {
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

    /// Find all possible text ranges for this selector
    pub fn find_text_ranges(&self, source_code: &str) -> Result<Vec<TextRange>, String> {
        log::trace!("top of find_text_ranges for {self:?}");

        self.validate()?;
        log::trace!("validated");

        match self {
            Selector::Insert { anchor, position } => {
                find_insert_positions(anchor, *position, source_code)
                    .map(|positions| positions.into_iter().map(TextRange::Insert).collect())
            }
            Selector::Replace { exact, from, to } => {
                if let Some(exact_text) = exact {
                    find_exact_matches(exact_text, source_code)
                        .map(|ranges| ranges.into_iter().map(TextRange::Replace).collect())
                } else if let Some(from_text) = from {
                    find_range_matches(from_text, to.as_deref(), source_code)
                        .map(|ranges| ranges.into_iter().map(TextRange::Replace).collect())
                } else {
                    // This should be caught by validate(), but just in case
                    Err("Invalid replace operation".to_string())
                }
            }
        }
    }

    pub fn operation_name(&self) -> &str {
        match self {
            Selector::Insert { .. } => "Insert",
            Selector::Replace { .. } => "Replace",
        }
    }
}

#[derive(Debug, Clone)]
pub enum TextRange {
    Insert(InsertTextPosition),
    Replace(ReplaceTextPosition),
}

#[derive(Debug, Clone)]
pub struct InsertTextPosition {
    pub byte_offset: usize,
    pub anchor: String,
    pub position: InsertPosition,
}

#[derive(Debug, Clone)]
pub struct ReplaceTextPosition {
    pub start_byte: usize,
    pub end_byte: usize,
    pub matched_text: String,
    pub replace_type: ReplaceType,
}

#[derive(Debug, Clone)]
pub enum ReplaceType {
    Exact { text: String },
    Range { from: String, to: Option<String> },
}

impl TextRange {
    /// Apply this edit to the source code and return the modified result
    pub fn apply_edit(&self, source_code: &str, new_content: &str) -> String {
        let mut rope = Rope::from_str(source_code);

        match self {
            TextRange::Insert(insert) => {
                let char_idx = rope.byte_to_char(insert.byte_offset);
                rope.insert(char_idx, new_content);
            }
            TextRange::Replace(replace) => {
                let start_char = rope.byte_to_char(replace.start_byte);
                let end_char = rope.byte_to_char(replace.end_byte);
                rope.remove(start_char..end_char);
                rope.insert(start_char, new_content);
            }
        }

        rope.to_string()
    }

    /// Get a human-readable description of this text range
    pub fn description(&self) -> String {
        match self {
            TextRange::Insert(insert) => {
                format!(
                    "Insert {} anchor \"{}\"",
                    match insert.position {
                        InsertPosition::Before => "before",
                        InsertPosition::After => "after",
                    },
                    insert.anchor
                )
            }
            TextRange::Replace(replace) => match &replace.replace_type {
                ReplaceType::Exact { text } => {
                    format!("Replace exact match \"{text}\"")
                }
                ReplaceType::Range { from, to } => {
                    if let Some(to_text) = to {
                        format!("Replace range from \"{from}\" to \"{to_text}\"")
                    } else {
                        format!("Replace from \"{from}\" (structural)")
                    }
                }
            },
        }
    }
}

fn find_insert_positions(
    anchor: &str,
    position: InsertPosition,
    source_code: &str,
) -> Result<Vec<InsertTextPosition>, String> {
    log::trace!("top of find_insert_positions for {anchor:?}, {position:?}");

    let positions = source_code
        .match_indices(anchor)
        .map(|(byte_offset, _)| {
            let adjusted_offset = match position {
                InsertPosition::Before => byte_offset,
                InsertPosition::After => byte_offset + anchor.len(),
            };
            InsertTextPosition {
                byte_offset: adjusted_offset,
                anchor: anchor.to_string(),
                position,
            }
        })
        .collect::<Vec<_>>();

    if positions.is_empty() {
        Err(format!("Anchor text \"{anchor}\" not found in source"))
    } else {
        Ok(positions)
    }
}

fn find_exact_matches(
    exact_text: &str,
    source_code: &str,
) -> Result<Vec<ReplaceTextPosition>, String> {
    let positions = source_code
        .match_indices(exact_text)
        .map(|(start_byte, matched)| ReplaceTextPosition {
            start_byte,
            end_byte: start_byte + matched.len(),
            matched_text: matched.to_string(),
            replace_type: ReplaceType::Exact {
                text: exact_text.to_string(),
            },
        })
        .collect::<Vec<_>>();

    if positions.is_empty() {
        Err(format!("Exact text \"{exact_text}\" not found in source"))
    } else {
        Ok(positions)
    }
}

fn find_range_matches(
    from_text: &str,
    to_text: Option<&str>,
    source_code: &str,
) -> Result<Vec<ReplaceTextPosition>, String> {
    let from_positions: Vec<_> = source_code.match_indices(from_text).collect();

    if from_positions.is_empty() {
        return Err(format!("From text \"{from_text}\" not found in source"));
    }

    if let Some(to_text) = to_text {
        // Explicit range: find from...to pairs
        let to_positions: Vec<_> = source_code.match_indices(to_text).collect();

        if to_positions.is_empty() {
            return Err(format!("To text \"{to_text}\" not found in source"));
        }

        let mut ranges = Vec::new();

        for (from_byte, _) in from_positions {
            // Find the first 'to' position that comes after this 'from' position
            // Use outer edges: start of 'from' to end of 'to'
            for (to_byte, _) in &to_positions {
                if *to_byte >= from_byte + from_text.len() {
                    let start_byte = from_byte;
                    let end_byte = *to_byte + to_text.len();

                    ranges.push(ReplaceTextPosition {
                        start_byte,
                        end_byte,
                        matched_text: source_code[start_byte..end_byte].to_string(),
                        replace_type: ReplaceType::Range {
                            from: from_text.to_string(),
                            to: Some(to_text.to_string()),
                        },
                    });
                    break; // Take the first valid 'to' for this 'from'
                }
            }
        }

        if ranges.is_empty() {
            Err(format!(
                "No valid range found from \"{from_text}\" to \"{to_text}\""
            ))
        } else {
            Ok(ranges)
        }
    } else {
        Err("structural replacement not implemented yet, please provide a `to` snippet".into())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_insert_before() {
        let selector = Selector::Insert {
            anchor: "fn main()".to_string(),
            position: InsertPosition::Before,
        };

        let source = "fn main() {\n    println!(\"Hello\");\n}";
        let ranges = selector.find_text_ranges(source).unwrap();

        assert_eq!(ranges.len(), 1);
        match &ranges[0] {
            TextRange::Insert(insert) => {
                assert_eq!(insert.byte_offset, 0);
                assert_eq!(insert.anchor, "fn main()");
            }
            _ => panic!("Expected insert range"),
        }
    }

    #[test]
    fn test_insert_after() {
        let selector = Selector::Insert {
            anchor: "fn main()".to_string(),
            position: InsertPosition::After,
        };

        let source = "fn main() {\n    println!(\"Hello\");\n}";
        let ranges = selector.find_text_ranges(source).unwrap();

        assert_eq!(ranges.len(), 1);
        match &ranges[0] {
            TextRange::Insert(insert) => {
                assert_eq!(insert.byte_offset, 9); // After "fn main()"
            }
            _ => panic!("Expected insert range"),
        }
    }

    #[test]
    fn test_exact_replace() {
        let selector = Selector::Replace {
            exact: Some("println!".to_string()),
            from: None,
            to: None,
        };

        let source = "fn main() {\n    println!(\"Hello\");\n}";
        let ranges = selector.find_text_ranges(source).unwrap();

        assert_eq!(ranges.len(), 1);
        match &ranges[0] {
            TextRange::Replace(replace) => {
                assert_eq!(replace.start_byte, 16);
                assert_eq!(replace.end_byte, 24);
                assert_eq!(replace.matched_text, "println!");
            }
            _ => panic!("Expected replace range"),
        }
    }

    #[test]
    fn test_range_replace() {
        let selector = Selector::Replace {
            exact: None,
            from: Some("fn main() {".to_string()),
            to: Some("}".to_string()),
        };

        let source = "fn main() {\n    println!(\"Hello\");\n}";
        let ranges = selector.find_text_ranges(source).unwrap();

        assert_eq!(ranges.len(), 1);
        match &ranges[0] {
            TextRange::Replace(replace) => {
                assert_eq!(replace.start_byte, 0);
                assert_eq!(replace.end_byte, source.len());
            }
            _ => panic!("Expected replace range"),
        }
    }

    #[test]
    fn test_validation_errors() {
        // Both exact and from specified
        let selector = Selector::Replace {
            exact: Some("test".to_string()),
            from: Some("other".to_string()),
            to: None,
        };
        assert!(selector.validate().is_err());

        // Neither exact nor from specified
        let selector = Selector::Replace {
            exact: None,
            from: None,
            to: None,
        };
        assert!(selector.validate().is_err());

        // Exact with to specified
        let selector = Selector::Replace {
            exact: Some("test".to_string()),
            from: None,
            to: Some("end".to_string()),
        };
        assert!(selector.validate().is_err());
    }
}
