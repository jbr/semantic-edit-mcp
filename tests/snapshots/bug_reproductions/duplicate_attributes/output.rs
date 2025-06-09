use std::collections::HashMap;

#[test]
fn existing_test() {
    assert_eq!(2 + 2, 4);
}

pub fn regular_function() {
    println!("Regular function");
}

#[tokio::test]
#[tokio::test]
async fn async_test() {
    // Modified async test with debug
    println!("Debug output");
    assert!(true);
}
