#[main]
pub fn main() {
    match std::env::var("x") {
        Ok(var) => {
            println!("{}", var.to_ascii_uppercase());
        }

        Err(e) => {
            eprintln!("error: {e}");
        }
    }
}

pub fn other() {
    let my_var = "var";
}
