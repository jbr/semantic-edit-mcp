// Test file for get_node_info
pub fn sample_function(param: i32) -> String {
    format!("Value: {}", param)
}

pub struct SampleStruct {
    field1: i32,
    field2: String,
}

impl SampleStruct {
    pub fn method(&self) -> i32 {
        self.field1
    }
}
