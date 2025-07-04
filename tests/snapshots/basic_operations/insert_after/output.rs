// Test file for insert after function
use std::collections::HashMap;

pub fn existing_function() -> i32 {
    println!("Existing function");
    eprintln!("here");
    42
}

pub struct TestStruct {
    value: i32,
}

impl TestStruct {
    pub fn new(value: i32) -> Self {
        Self { value }
    }
}
