pub mod rust;

use anyhow::{Result, anyhow};
use std::collections::HashMap;
use tree_sitter::{Node, Parser, Tree};

pub struct TreeSitterParser {
    parsers: HashMap<String, Parser>,
}

impl TreeSitterParser {
    pub fn new() -> Result<Self> {
        let mut parsers = HashMap::new();

        // Initialize Rust parser
        let mut rust_parser = Parser::new();
        rust_parser.set_language(&tree_sitter_rust::LANGUAGE.into())?;
        parsers.insert("rust".to_string(), rust_parser);

        // TODO: Add more languages as needed
        // let mut ts_parser = Parser::new();
        // ts_parser.set_language(&tree_sitter_typescript::language())?;
        // parsers.insert("typescript".to_string(), ts_parser);

        Ok(Self { parsers })
    }

    pub fn parse(&mut self, language: &str, source_code: &str) -> Result<Tree> {
        let parser = self
            .parsers
            .get_mut(language)
            .ok_or_else(|| anyhow!("Unsupported language: {}", language))?;

        parser
            .parse(source_code, None)
            .ok_or_else(|| anyhow!("Failed to parse {} code", language))
    }

    pub fn supported_languages(&self) -> Vec<&String> {
        self.parsers.keys().collect()
    }
}

pub fn detect_language_from_path(file_path: &str) -> Option<String> {
    if let Some(extension) = std::path::Path::new(file_path).extension() {
        match extension.to_str()? {
            "rs" => Some("rust".to_string()),
            "ts" | "tsx" => Some("typescript".to_string()),
            "js" | "jsx" => Some("javascript".to_string()),
            "py" => Some("python".to_string()),
            _ => None,
        }
    } else {
        None
    }
}

pub fn get_node_text<'a>(node: &Node, source_code: &'a str) -> &'a str {
    &source_code[node.start_byte()..node.end_byte()]
}

pub fn find_node_by_position(tree: &Tree, line: usize, column: usize) -> Option<Node> {
    let point = tree_sitter::Point::new(line.saturating_sub(1), column.saturating_sub(1));
    let mut node = tree.root_node().descendant_for_point_range(point, point)?;

    // Walk up the tree to find a more "meaningful" node for editing
    // Skip trivial nodes like punctuation, identifiers, and literals
    while is_trivial_node(&node) && node.parent().is_some() {
        if let Some(parent) = node.parent() {
            node = parent;
        } else {
            break;
        }
    }

    Some(node)
}

/// Determines if a node is "trivial" and should be skipped for semantic selection
fn is_trivial_node(node: &Node) -> bool {
    match node.kind() {
        // Skip punctuation and delimiters
        "(" | ")" | "{" | "}" | "[" | "]" | ";" | "," | "." | ":" | "::" | "!" => true,
        // Skip small tokens that are rarely useful for editing
        "identifier" | "string_content" | "integer_literal" | "float_literal" => true,
        // Skip keywords unless they're at the start of a meaningful construct
        "fn" | "struct" | "impl" | "let" | "mut" | "pub" => {
            // Only skip if parent exists and is more meaningful
            node.parent().is_some_and(|parent| {
                matches!(
                    parent.kind(),
                    "function_item" | "struct_item" | "impl_item" | "let_declaration"
                )
            })
        }
        _ => false,
    }
}
