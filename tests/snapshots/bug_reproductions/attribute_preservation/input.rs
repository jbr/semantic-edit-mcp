use std::collections::HashMap;

#[test]
fn existing_test() {
    assert_eq!(2 + 2, 4);
}

pub fn regular_function() {
    println!("Regular function");
}

#[tokio::test]
async fn async_test() {
    // Original async test with tokio::test attribute
    assert!(true);
}

#[derive(Debug, Clone)]
struct TestStruct {
    value: i32,
}
