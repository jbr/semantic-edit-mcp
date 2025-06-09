use anyhow::Result;

pub struct TestStruct {
    value: i32,
}

impl TestStruct {
    pub fn new(value: i32) -> Self {
        Self { value }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_functionality() {
        let instance = TestStruct::new(42);
        assert_eq!(instance.value, 42);
    }
}

// Helper function with attributes
#[inline]
#[must_use]
pub fn helper_function() -> String {
    "original implementation".to_string()
}

pub fn regular_function() {
    println!("This function has no attributes");
}
