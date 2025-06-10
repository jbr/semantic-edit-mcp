use anyhow::Result;
use tree_sitter::{Node, Query, QueryCursor, StreamingIterator, Tree};

pub struct RustParser;

impl RustParser {
    pub fn find_function_by_name<'a>(
        tree: &'a Tree,
        source_code: &str,
        function_name: &str,
    ) -> Result<Option<Node<'a>>> {
        let query_text = format!(
            r#"
            (function_item
                name: (identifier) @name
                (#eq? @name "{function_name}")) @function
            "#
        );

        let query = Query::new(&tree_sitter_rust::LANGUAGE.into(), &query_text)?;
        let mut cursor = QueryCursor::new();

        let mut matches = cursor.matches(&query, tree.root_node(), source_code.as_bytes());
        while let Some(m) = matches.next() {
            for capture in m.captures {
                if capture.index == query.capture_index_for_name("function").unwrap() {
                    return Ok(Some(capture.node));
                }
            }
        }

        Ok(None)
    }

    pub fn find_struct_by_name<'a>(
        tree: &'a Tree,
        source_code: &str,
        struct_name: &str,
    ) -> Result<Option<Node<'a>>> {
        let query_text = format!(
            r#"
            (struct_item
                name: (type_identifier) @name
                (#eq? @name "{struct_name}")) @struct
            "#
        );

        let query = Query::new(&tree_sitter_rust::LANGUAGE.into(), &query_text)?;
        let mut cursor = QueryCursor::new();

        let mut matches = cursor.matches(&query, tree.root_node(), source_code.as_bytes());
        while let Some(m) = matches.next() {
            for capture in m.captures {
                if capture.index == query.capture_index_for_name("struct").unwrap() {
                    return Ok(Some(capture.node));
                }
            }
        }

        Ok(None)
    }

    pub fn find_enum_by_name<'a>(
        tree: &'a Tree,
        source_code: &str,
        enum_name: &str,
    ) -> Result<Option<Node<'a>>> {
        let query_text = format!(
            r#"
            (enum_item
                name: (type_identifier) @name
                (#eq? @name "{enum_name}")) @enum
            "#
        );

        let query = Query::new(&tree_sitter_rust::LANGUAGE.into(), &query_text)?;
        let mut cursor = QueryCursor::new();

        let mut matches = cursor.matches(&query, tree.root_node(), source_code.as_bytes());
        while let Some(m) = matches.next() {
            for capture in m.captures {
                if capture.index == query.capture_index_for_name("enum").unwrap() {
                    return Ok(Some(capture.node));
                }
            }
        }

        Ok(None)
    }

    pub fn find_impl_block_for_type<'a>(
        tree: &'a Tree,
        source_code: &str,
        type_name: &str,
    ) -> Result<Vec<Node<'a>>> {
        let query_text = format!(
            r#"
            (impl_item
                type: (type_identifier) @type_name
                (#eq? @type_name "{type_name}")) @impl
            "#
        );

        let query = Query::new(&tree_sitter_rust::LANGUAGE.into(), &query_text)?;
        let mut cursor = QueryCursor::new();
        let mut results = Vec::new();

        let mut matches = cursor.matches(&query, tree.root_node(), source_code.as_bytes());
        while let Some(m) = matches.next() {
            for capture in m.captures {
                if capture.index == query.capture_index_for_name("impl").unwrap() {
                    results.push(capture.node);
                }
            }
        }

        Ok(results)
    }

    pub fn find_nodes_by_type<'a>(tree: &'a Tree, node_type: &str) -> Vec<Node<'a>> {
        let mut results = Vec::new();

        fn traverse<'a>(node: Node<'a>, node_type: &str, results: &mut Vec<Node<'a>>) {
            if node.kind() == node_type {
                results.push(node);
            }

            for child in node.children(&mut node.walk()) {
                traverse(child, node_type, results);
            }
        }

        traverse(tree.root_node(), node_type, &mut results);
        results
    }

    pub fn validate_rust_syntax(source_code: &str) -> Result<bool> {
        let mut parser = tree_sitter::Parser::new();
        parser.set_language(&tree_sitter_rust::LANGUAGE.into())?;

        if let Some(tree) = parser.parse(source_code, None) {
            Ok(!tree.root_node().has_error())
        } else {
            Ok(false)
        }
    }

    pub fn get_all_function_names(tree: &Tree, source_code: &str) -> Vec<String> {
        let query_text = r#"
            (function_item
                name: (identifier) @name) @function
        "#;

        if let Ok(query) = tree_sitter::Query::new(&tree_sitter_rust::LANGUAGE.into(), query_text) {
            let mut cursor = tree_sitter::QueryCursor::new();
            let mut names = Vec::new();

            let mut matches = cursor.matches(&query, tree.root_node(), source_code.as_bytes());
            while let Some(m) = matches.next() {
                for capture in m.captures {
                    if let Some(name_index) = query.capture_index_for_name("name") {
                        if capture.index == name_index {
                            let name_text =
                                &source_code[capture.node.start_byte()..capture.node.end_byte()];
                            names.push(name_text.to_string());
                        }
                    }
                }
            }
            names
        } else {
            Vec::new()
        }
    }

    pub fn get_all_struct_names(tree: &Tree, source_code: &str) -> Vec<String> {
        let query_text = r#"
            (struct_item
                name: (type_identifier) @name) @struct
        "#;

        if let Ok(query) = tree_sitter::Query::new(&tree_sitter_rust::LANGUAGE.into(), query_text) {
            let mut cursor = tree_sitter::QueryCursor::new();
            let mut names = Vec::new();

            let mut matches = cursor.matches(&query, tree.root_node(), source_code.as_bytes());
            while let Some(m) = matches.next() {
                for capture in m.captures {
                    if let Some(name_index) = query.capture_index_for_name("name") {
                        if capture.index == name_index {
                            let name_text =
                                &source_code[capture.node.start_byte()..capture.node.end_byte()];
                            names.push(name_text.to_string());
                        }
                    }
                }
            }
            names
        } else {
            Vec::new()
        }
    }
    pub fn get_all_enum_names(tree: &Tree, source_code: &str) -> Vec<String> {
        let query_text = r#"
            (enum_item
                name: (type_identifier) @name) @enum
        "#;

        if let Ok(query) = tree_sitter::Query::new(&tree_sitter_rust::LANGUAGE.into(), query_text) {
            let mut cursor = tree_sitter::QueryCursor::new();
            let mut names = Vec::new();

            let mut matches = cursor.matches(&query, tree.root_node(), source_code.as_bytes());
            while let Some(m) = matches.next() {
                for capture in m.captures {
                    if let Some(name_index) = query.capture_index_for_name("name") {
                        if capture.index == name_index {
                            let name_text =
                                &source_code[capture.node.start_byte()..capture.node.end_byte()];
                            names.push(name_text.to_string());
                        }
                    }
                }
            }
            names
        } else {
            Vec::new()
        }
    }

    pub fn get_all_impl_types(tree: &Tree, source_code: &str) -> Vec<String> {
        let query_text = r#"
            (impl_item
                type: (type_identifier) @type_name) @impl
        "#;

        if let Ok(query) = tree_sitter::Query::new(&tree_sitter_rust::LANGUAGE.into(), query_text) {
            let mut cursor = tree_sitter::QueryCursor::new();
            let mut names = Vec::new();

            let mut matches = cursor.matches(&query, tree.root_node(), source_code.as_bytes());
            while let Some(m) = matches.next() {
                for capture in m.captures {
                    if let Some(name_index) = query.capture_index_for_name("type_name") {
                        if capture.index == name_index {
                            let name_text =
                                &source_code[capture.node.start_byte()..capture.node.end_byte()];
                            names.push(name_text.to_string());
                        }
                    }
                }
            }
            names
        } else {
            Vec::new()
        }
    }

    pub fn get_all_mod_names(tree: &Tree, source_code: &str) -> Vec<String> {
        let query_text = r#"
            (mod_item
                name: (identifier) @name) @mod
        "#;

        if let Ok(query) = tree_sitter::Query::new(&tree_sitter_rust::LANGUAGE.into(), query_text) {
            let mut cursor = tree_sitter::QueryCursor::new();
            let mut names = Vec::new();

            let mut matches = cursor.matches(&query, tree.root_node(), source_code.as_bytes());
            while let Some(m) = matches.next() {
                for capture in m.captures {
                    if let Some(name_index) = query.capture_index_for_name("name") {
                        if capture.index == name_index {
                            let name_text =
                                &source_code[capture.node.start_byte()..capture.node.end_byte()];
                            names.push(name_text.to_string());
                        }
                    }
                }
            }
            names
        } else {
            Vec::new()
        }
    }

    /// Helper function that tries to find nodes by type, with automatic fallback for common Rust patterns.
    /// Follows the "user is never wrong" principle by accepting intuitive type names.
    ///
    /// Examples:
    /// - "struct" → tries "struct", then "struct_item"
    /// - "function" → tries "function", then "function_item"
    /// - "enum" → tries "enum", then "enum_item"
    pub fn find_nodes_by_type_with_fallback<'a>(tree: &'a Tree, user_type: &str) -> Vec<Node<'a>> {
        // First, try the user's exact input
        let mut results = Self::find_nodes_by_type(tree, user_type);

        // If nothing found and this looks like a Rust top-level construct, try with _item suffix
        if results.is_empty() {
            let item_type = match user_type {
                "struct" => Some("struct_item"),
                "function" | "fn" => Some("function_item"),
                "enum" => Some("enum_item"),
                "impl" => Some("impl_item"),
                "mod" | "module" => Some("mod_item"),
                "use" => Some("use_declaration"),
                "type" => Some("type_item"),
                "trait" => Some("trait_item"),
                "const" => Some("const_item"),
                "static" => Some("static_item"),
                _ => None,
            };

            if let Some(fallback_type) = item_type {
                results = Self::find_nodes_by_type(tree, fallback_type);
            }
        }

        results
    }

    /// Enhanced find by name that accepts both user-friendly and exact type names
    pub fn find_by_name_with_fallback<'a>(
        tree: &'a Tree,
        source_code: &str,
        user_type: &str,
        name: &str,
    ) -> Result<Option<Node<'a>>> {
        // Try exact type first
        let result = match user_type {
            "function_item" | "function" | "fn" => {
                Self::find_function_by_name(tree, source_code, name)
            }
            "struct_item" | "struct" => Self::find_struct_by_name(tree, source_code, name),
            "enum_item" | "enum" => Self::find_enum_by_name(tree, source_code, name),
            _ => {
                // For unknown types, try the user input first, then with _item suffix
                let nodes_by_exact = Self::find_nodes_by_type(tree, user_type);
                let nodes_by_fallback = if !user_type.ends_with("_item") {
                    Self::find_nodes_by_type(tree, &format!("{user_type}_item"))
                } else {
                    Vec::new()
                };

                // Combine results and find by name manually
                let all_nodes = [nodes_by_exact, nodes_by_fallback].concat();
                for node in all_nodes {
                    if let Some(name_node) = Self::get_name_node(node) {
                        let node_name = &source_code[name_node.start_byte()..name_node.end_byte()];
                        if node_name == name {
                            return Ok(Some(node));
                        }
                    }
                }
                return Ok(None);
            }
        };

        result
    }

    /// Helper to extract the name node from various Rust constructs
    fn get_name_node<'a>(node: Node<'a>) -> Option<Node<'a>> {
        match node.kind() {
            "function_item" => {
                // Function name is usually the second child (after 'fn' keyword)
                for child in node.children(&mut node.walk()) {
                    if child.kind() == "identifier" {
                        return Some(child);
                    }
                }
            }
            "struct_item" | "enum_item" | "trait_item" | "type_item" => {
                // Type name is usually a type_identifier
                for child in node.children(&mut node.walk()) {
                    if child.kind() == "type_identifier" {
                        return Some(child);
                    }
                }
            }
            "impl_item" => {
                // Implementation target type
                for child in node.children(&mut node.walk()) {
                    if child.kind() == "type_identifier" {
                        return Some(child);
                    }
                }
            }
            "mod_item" => {
                // Module name
                for child in node.children(&mut node.walk()) {
                    if child.kind() == "identifier" {
                        return Some(child);
                    }
                }
            }
            _ => {}
        }
        None
    }
}
