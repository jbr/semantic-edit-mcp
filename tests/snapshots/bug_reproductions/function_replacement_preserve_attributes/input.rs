use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct DataProcessor {
    data: HashMap<String, i32>,
}

impl DataProcessor {
    pub fn new() -> Self {
        Self {
            data: HashMap::new(),
        }
    }
}

#[cfg(feature = "async")]
#[tokio::main]
async fn main() {
    println!("Async main function");
}

#[deprecated(since = "1.0.0", note = "Use new_process instead")]
#[allow(dead_code)]
pub fn old_process(input: &str) -> String {
    format!("Processing: {}", input)
}

pub fn simple_function() {
    println!("No attributes here");
}
