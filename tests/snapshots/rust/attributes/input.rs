use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestStruct {
    pub field: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TestEnum {
    Variant1,
    Variant2,
}
