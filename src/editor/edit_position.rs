use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug, Copy, Eq, PartialEq)]
pub struct EditPosition {
    pub start_byte: usize,
    pub end_byte: Option<usize>, // None for insert, Some for replace
}
