// Test file to verify that replacing attributed functions doesn't duplicate attributes

#[test]
fn simple_test() {
    assert_eq!(1 + 1, 2);
}

#[tokio::test]
async fn original_async_test() {
    println!("Original test");
    assert!(true);
}

#[derive(Debug)]
pub struct TestStruct {
    value: i32,
}
