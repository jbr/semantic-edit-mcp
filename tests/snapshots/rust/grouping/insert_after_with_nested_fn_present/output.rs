mod tests {
    /// First test.
    #[test]
    fn first() {
        assert!(true);
    }
    /// Inserted test.
    #[test]
    fn inserted() {
        assert!(true);
    }

    /// A helper that defines a valid nested function.
    fn uses_nested_helper() {
        fn helper(x: i32) -> i32 {
            x + 1
        }
        let _ = helper(1);
    }
}
