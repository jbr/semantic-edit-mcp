fn main() {
    match std::env::arg("SOME_ARG") {
        Ok("some_value") => {
            println!("some value!")
        }

        Ok("other_value") => {
            println!("other value!")
        }

        _ => {}
    }
}
