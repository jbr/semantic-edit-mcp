use semantic_edit_mcp::validation::ContextValidator;
use semantic_edit_mcp::parsers::TreeSitterParser;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Test case 1: Valid placement - function at module level
    let valid_rust_code = r#"
struct Point {
    x: i32,
    y: i32,
}

// This is where we want to insert a function - valid placement
"#;

    let function_content = r#"fn distance(p1: &Point, p2: &Point) -> f64 {
    let dx = (p1.x - p2.x) as f64;
    let dy = (p1.y - p2.y) as f64;
    (dx * dx + dy * dy).sqrt()
}"#;

    println!("🧪 Testing Tree-sitter Context Validation\n");

    // Initialize validator and parser
    let validator = ContextValidator::new()?;
    let mut parser = TreeSitterParser::new()?;

    // Test 1: Valid function insertion after struct
    println!("Test 1: Valid function placement after struct");
    let tree = parser.parse("rust", valid_rust_code)?;
    let root = tree.root_node();
    
    // Find the struct node
    let struct_node = root.children(&mut root.walk())
        .find(|n| n.kind() == "struct_item")
        .expect("Should find struct");

    let validation_result = validator.validate_insertion(
        &tree,
        valid_rust_code,
        &struct_node,
        function_content,
        "rust",
        &semantic_edit_mcp::validation::OperationType::InsertAfter,
    )?;

    if validation_result.is_valid {
        println!("✅ Valid placement detected correctly");
    } else {
        println!("❌ Valid placement incorrectly flagged as invalid:");
        println!("{}", validation_result.format_errors());
    }

    // Test 2: Invalid placement - trying to insert struct inside function
    println!("\nTest 2: Invalid struct placement inside function");
    let function_code = r#"
fn main() {
    let x = 5;
    // Trying to insert a struct here would be invalid
}
"#;
    
    let struct_content = "struct InvalidStruct { field: i32 }";
    
    let func_tree = parser.parse("rust", function_code)?;
    let func_root = func_tree.root_node();
    
    // Find the function node
    let function_node = func_root.children(&mut func_root.walk())
        .find(|n| n.kind() == "function_item")
        .expect("Should find function");

    let invalid_validation = validator.validate_insertion(
        &func_tree,
        function_code,
        &function_node,
        struct_content,
        "rust",
        &semantic_edit_mcp::validation::OperationType::InsertAfter,
    )?;

    if !invalid_validation.is_valid {
        println!("✅ Context validator caught invalid placement:");
        println!("{}", invalid_validation.format_errors());
    } else {
        println!("❌ Context validator failed to catch invalid placement");
    }

    println!("\n🎉 Tree-sitter context validation tests completed!");
    Ok(())
}
