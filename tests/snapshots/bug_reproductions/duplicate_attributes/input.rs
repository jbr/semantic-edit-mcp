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
    // Existing async test
    assert!(true);
}
