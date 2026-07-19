/// Parses a response, using a struct scoped to this one fn — legal, idiomatic
/// Rust that must never make the rest of the file uneditable.
fn parse(input: &str) -> usize {
    struct Local {
        value: usize,
    }
    impl Local {
        fn get(&self) -> usize {
            self.value
        }
    }
    let local = Local { value: input.len() };
    local.get()
}

/// Doc for target.
fn target() {}
