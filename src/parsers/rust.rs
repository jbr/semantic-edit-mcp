use anyhow::Result;
use tree_sitter::{Node, Query, QueryCursor, Tree};

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

        for m in cursor.matches(&query, tree.root_node(), source_code.as_bytes()) {
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

        for m in cursor.matches(&query, tree.root_node(), source_code.as_bytes()) {
            for capture in m.captures {
                if capture.index == query.capture_index_for_name("struct").unwrap() {
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

        for m in cursor.matches(&query, tree.root_node(), source_code.as_bytes()) {
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
}
