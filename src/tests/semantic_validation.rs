use crate::{
    editor::Editor,
    languages::{LanguageName, LanguageRegistry},
};

#[test]
fn impl_block_pub_fn() {
    assert!(validate_code(r#"impl User pub fn new () {}"#, LanguageName::Rust).is_some());
}

#[test]
fn async_await_is_valid() {
    // Regression (efference dogfood failure): the validator flagged every `.await`
    // as `await.outside.async` because the query's `#not-has-ancestor?` guards are
    // custom predicates that tree-sitter never evaluates — so the rule degenerated
    // to "all awaits are invalid". Valid async code must produce no violation.
    let rust = r#"
async fn run() {
    let x = fetch().await;
    other().await;
}
"#;
    assert_eq!(validate_code(rust, LanguageName::Rust), None);
}

#[test]
fn mutable_static_in_unsafe_is_valid() {
    // Same root cause: `#not-has-ancestor? unsafe_block` was never evaluated, so a
    // mutable static initialized inside an `unsafe` block was wrongly flagged.
    let rust = r#"
fn main() {
    unsafe {
        static mut COUNTER: u32 = 0;
    }
}
"#;
    assert_eq!(validate_code(rust, LanguageName::Rust), None);
}

#[test]
fn free_fn_with_reference_param_not_named_self_is_valid() {
    // Locks in the `#eq? @self_param "self"` text-predicate fix. Before predicates
    // were applied, the `invalid.manual.self.outside.impl` rule degenerated to "any
    // free fn whose first param has a reference type", flagging ordinary functions.
    let rust = "fn handler(req: &Request) {}\n";
    assert_eq!(validate_code(rust, LanguageName::Rust), None);
}

#[test]
fn method_with_self_inside_impl_is_valid() {
    // Locks in the `#not-has-ancestor? impl_item` ancestor-predicate fix: a `self`
    // method that *is* inside an impl must not be flagged.
    let rust = "struct S;\nimpl S {\n    fn f(self: &S) {}\n}\n";
    assert_eq!(validate_code(rust, LanguageName::Rust), None);
}

#[test]
fn module_level_static_mut_is_valid() {
    // Declaring a mutable static is legal anywhere; only *access* requires unsafe.
    // The old `mut.static.without.unsafe` rule flagged the declaration, which (via
    // prevalidate) blocked editing any file that contained one.
    assert_eq!(
        validate_code("static mut COUNTER: u32 = 0;\n", LanguageName::Rust),
        None
    );
}

#[test]
fn const_fn_with_mut_ref_local_is_valid() {
    // `&mut` in a const fn has been valid since `const_mut_refs` (Rust 1.83).
    let rust = "const fn f(p: &mut u32) {\n    let _r: &mut u32 = p;\n}\n";
    assert_eq!(validate_code(rust, LanguageName::Rust), None);
}

#[test]
fn docs_automation() {
    assert_eq!(
        validate_code(
            include_str!("../../tests/semantic_validation_corpus/docs_automation.rs"),
            LanguageName::Rust
        ),
        None
    );
}

mod python {
    use super::*;
    #[test]
    fn corpus_is_ok() {
        assert_eq!(
            validate_code(
                include_str!("../../tests/semantic_validation_corpus/example.py"),
                LanguageName::Python
            ),
            None
        );
    }

    #[test]
    fn no_self_outside_class() {
        assert!(
            validate_code(
                r#"def method_with_self(self):\n    return "this has self but is outside class"#,
                LanguageName::Python
            )
            .is_some()
        );
    }

    #[test]
    fn module_level_fn_with_non_self_param_is_valid() {
        // Locks in the `#eq? @self_param "self"` text-predicate fix on the Python
        // side: before predicates were applied, *any* module-level function with a
        // first parameter was flagged as a "self method at module level".
        assert_eq!(
            validate_code("def foo(x):\n    return x\n", LanguageName::Python),
            None
        );
    }

    #[test]
    fn return_at_module() {
        assert!(validate_code(r#"print("hello")\nreturn 42"#, LanguageName::Python).is_some());
    }

    #[test]
    fn yield_at_module() {
        assert!(validate_code(r#"x = 1\nyield 42"#, LanguageName::Python).is_some());
    }

    #[test]
    fn class_in_function() {
        let python = r#"
def my_function():
    class NestedClass:
        pass
    return NestedClass()
"#;

        assert!(validate_code(python, LanguageName::Python).is_some());
    }
}

fn validate_code(code: &str, language: LanguageName) -> Option<String> {
    let registry = LanguageRegistry::new().unwrap();
    let language = registry.get_language(language);
    let mut parser = language.tree_sitter_parser().unwrap();
    let tree = parser.parse(code, None).unwrap();
    println!("{}", tree.root_node());
    let result = Editor::validate(language, &tree, code)?;
    println!("{result}");
    Some(result)
}
