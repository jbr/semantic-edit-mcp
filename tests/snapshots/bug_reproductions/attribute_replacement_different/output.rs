use std::collections::HashMap;

#[test]
fn existing_test() {
    assert_eq!(2 + 2, 4);
}

pub fn regular_function() {
    println!("Regular function");
}

#[test]
fn async_test() {
    // Changed from #[tokio::test] async fn to #[test] fn
    // Should replace the attribute, not duplicate
    println!("Now a regular test");
    assert!(true);
}

#[derive(Debug, Clone)]
struct TestStruct {
    value: i32,
}
