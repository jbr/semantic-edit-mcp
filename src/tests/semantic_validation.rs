use crate::{
    editor::Editor,
    languages::{LanguageName, LanguageRegistry},
};

#[test]
fn impl_block_pub_fn() {
    assert!(validate_code(r#"impl User pub fn new () {}"#, LanguageName::Rust).is_some());
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
    println!("{}", &tree.root_node().to_string());
    let result = Editor::validate(language, &tree, code)?;
    println!("{result}");
    Some(result)
}
