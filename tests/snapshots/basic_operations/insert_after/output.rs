// Test file for insert after function
use std::collections::HashMap;

pub fn existing_function() -> i32 {
    println!("Existing function");
    42
}
pub fn new_function() -> String {
    "inserted after existing_function".to_string()
}

pub struct TestStruct {
    value: i32,
}

impl TestStruct {
    pub fn new(value: i32) -> Self {
        Self { value }
    }
}
