use crate::languages::rust::RustLanguage;
use crate::languages::semantic_grouping::{SemanticGrouping, WithSemanticGrouping};
use crate::languages::LanguageSupport;

#[test]
fn test_rust_semantic_grouping() {
    let rust_code = r#"
// This is a comment about the function
#[derive(Debug)]
#[cfg(test)]
fn test_function() {
    println!("Hello world");
}

fn another_function() {
    println!("Another function");
}
"#;
    let rust_lang = RustLanguage::new().expect("Failed to create Rust language");
    let mut parser = rust_lang.tree_sitter_parser();
    let tree = parser
        .parse(rust_code, None)
        .expect("Failed to parse Rust code");

    // Find the first function
    let root = tree.root_node();
    let mut cursor = root.walk();

    // Navigate to find function_item nodes
    cursor.goto_first_child(); // source_file -> first child
    let mut function_node = None;

    loop {
        let node = cursor.node();
        if node.kind() == "function_item" {
            function_node = Some(node);
            break;
        }
        if !cursor.goto_next_sibling() {
            break;
        }
    }

    let function_node = function_node.expect("Should find a function node");

    // Test semantic grouping
    let group = rust_lang
        .find_semantic_group(&tree, function_node)
        .expect("Should find semantic group");

    println!("Function node: {:?}", function_node.kind());
    println!(
        "Function text: {}",
        &rust_code[function_node.start_byte()..function_node.end_byte()]
    );

    if group.has_preceding_elements() {
        println!("Found {} preceding elements:", group.preceding_nodes.len());
        for (i, node) in group.preceding_nodes.iter().enumerate() {
            let text = &rust_code[node.start_byte()..node.end_byte()];
            println!("  {}: {} - {}", i, node.kind(), text.trim());
        }

        let (group_start, group_end) = group.byte_range();
        println!("Group text: {}", &rust_code[group_start..group_end]);
    } else {
        println!("No preceding elements found");
    }

    // Test replacement range calculation
    let replacement_with_attributes = "#[test]\nfn new_function() { }";
    let replacement_without_attributes = "fn new_function() { }";

    let range_with_attrs = rust_lang
        .calculate_replacement_range(&tree, function_node, replacement_with_attributes, rust_code)
        .expect("Should calculate range with attrs");
    let range_without_attrs = rust_lang
        .calculate_replacement_range(
            &tree,
            function_node,
            replacement_without_attributes,
            rust_code,
        )
        .expect("Should calculate range without attrs");

    println!("Range with attributes: {:?}", range_with_attrs);
    println!("Range without attributes: {:?}", range_without_attrs);

    // The range with attributes should include the preceding elements
    // The range without attributes should preserve them
    if group.has_preceding_elements() {
        let group_range = group.byte_range();
        assert_eq!(
            range_with_attrs, group_range,
            "With attributes should replace whole group"
        );
        assert_eq!(
            range_without_attrs,
            (function_node.start_byte(), function_node.end_byte()),
            "Without attributes should preserve preceding elements"
        );
    }
}

#[test]
fn test_grouping_rule_creation() {
    use crate::languages::semantic_grouping::GroupingRule;

    let rule = GroupingRule::new("function_item")
        .with_preceding_types(vec!["attribute_item", "line_comment"])
        .with_max_gap_nodes(2);

    assert_eq!(rule.target_node_type, "function_item");
    assert_eq!(
        rule.preceding_node_types,
        vec!["attribute_item", "line_comment"]
    );
    assert_eq!(rule.max_gap_nodes, 2);
    assert!(rule.require_consecutive);
}

#[test]
fn test_replacement_content_detection() {
    let rust_lang = RustLanguage::new().expect("Failed to create Rust language");

    // Test detection of attributes
    assert!(rust_lang.replacement_has_preceding_elements("#[derive(Debug)]\nfn test() {}"));
    assert!(rust_lang.replacement_has_preceding_elements("  #[test]  \nfn test() {}"));

    // Test detection of comments
    assert!(rust_lang.replacement_has_preceding_elements("// Comment\nfn test() {}"));
    assert!(rust_lang.replacement_has_preceding_elements("/* Block comment */\nfn test() {}"));

    // Test non-matching content
    assert!(!rust_lang.replacement_has_preceding_elements("fn test() {}"));
    assert!(!rust_lang.replacement_has_preceding_elements("  fn test() {}"));
    assert!(!rust_lang.replacement_has_preceding_elements("pub fn test() {}"));
}
