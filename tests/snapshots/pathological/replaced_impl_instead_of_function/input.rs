struct TestStruct {
    value: i32,
}

impl TestStruct {
    fn new(value: i32) -> Self {
        Self { value }
    }

    fn get_value(&self) -> i32 {
        self.value
    }

    fn set_value(&mut self, new_value: i32) {
        self.value = new_value;
    }

    fn double_value(&self) -> i32 {
        self.value * 2
    }
}

fn standalone_function() -> String {
    "I'm not in an impl block".to_string()
}
