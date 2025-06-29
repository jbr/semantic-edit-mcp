use semantic_edit_mcp::{
    editor::Editor,
    languages::{LanguageName, LanguageRegistry},
};
use LanguageName::Rust;

#[test]
fn impl_block_pub_fn() {
    assert!(validate_code(r#"impl User pub fn new () {}"#, Rust).is_some());
}

fn validate_code(code: &str, language: LanguageName) -> Option<String> {
    let registry = LanguageRegistry::new().unwrap();
    let language = registry.get_language(language);
    let mut parser = language.tree_sitter_parser().unwrap();
    let tree = parser.parse(code, None).unwrap();
    println!("{}", &tree.root_node().to_string());
    Editor::validate(language, &tree, code)
}
