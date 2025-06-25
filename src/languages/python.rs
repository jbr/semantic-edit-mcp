use super::{utils::parse_node_types_json, LanguageCommon};
use crate::{
    languages::{traits::LanguageEditor, LanguageName},
    operations::{
        selector::Position::{After, Around, Before, Replace},
        EditOperation,
    },
};
use anyhow::{anyhow, Result};
use rustpython_parser::ast::TextSize;

pub fn language() -> Result<LanguageCommon> {
    let language = tree_sitter_python::LANGUAGE.into();
    let node_types = parse_node_types_json(tree_sitter_python::NODE_TYPES)?;
    let editor = Box::new(PythonEditor::new());

    Ok(LanguageCommon {
        name: LanguageName::Python,
        file_extensions: &["py", "pyi"],
        language,
        node_types,
        editor,
        validation_query: None,
    })
}

pub struct PythonEditor;

impl Default for PythonEditor {
    fn default() -> Self {
        Self::new()
    }
}

impl PythonEditor {
    pub fn new() -> Self {
        Self
    }

    // /// Detect the indentation level at a given position
    // fn detect_indentation(&self, source: &str, byte_pos: usize) -> String {
    //     let rope = ropey::Rope::from_str(source);
    //     let char_pos = rope.byte_to_char(byte_pos);
    //     let line_idx = rope.char_to_line(char_pos);

    //     if line_idx == 0 {
    //         return String::new();
    //     }

    //     // Look at the current line and previous lines to find indentation
    //     let line_start = rope.line_to_char(line_idx);
    //     let line_end = if line_idx + 1 < rope.len_lines() {
    //         rope.line_to_char(line_idx + 1)
    //     } else {
    //         rope.len_chars()
    //     };

    //     let line_content: String = rope.slice(line_start..line_end).to_string();

    //     // Extract leading whitespace
    //     let indent = line_content
    //         .chars()
    //         .take_while(|&c| c == ' ' || c == '\t')
    //         .collect::<String>();

    //     indent
    // }

    /// Check if we need to add a newline before content
    fn needs_newline_before(&self, node: tree_sitter::Node, content: &str) -> bool {
        if content.starts_with('\n') {
            return false; // Already has newline
        }

        match node.kind() {
            // Import statements should be separated by newlines
            "import_statement" | "import_from_statement" => true,
            // Function and class definitions need separation
            "function_definition" | "class_definition" => true,
            _ => false,
        }
    }

    /// Check if we need to add a newline after content
    fn needs_newline_after(&self, node: tree_sitter::Node, content: &str) -> bool {
        if content.ends_with('\n') {
            return false; // Already has newline
        }

        match node.kind() {
            // Import statements should be separated by newlines
            "import_statement" | "import_from_statement" => true,
            // Function and class definitions need trailing newlines
            "function_definition" | "class_definition" => true,
            _ => false,
        }
    }

    /// Format content for insertion with proper Python indentation and newlines
    fn format_content_for_insertion(
        &self,
        node: tree_sitter::Node,
        content: &str,
        position: &crate::operations::selector::Position,
    ) -> String {
        let mut formatted = content.to_string();

        match position {
            Before => {
                if self.needs_newline_before(node, content) && !formatted.starts_with('\n') {
                    formatted = format!("\n{formatted}");
                }
                if self.needs_newline_after(node, content) && !formatted.ends_with('\n') {
                    formatted.push('\n');
                }
            }
            After => {
                if self.needs_newline_after(node, content) && !formatted.starts_with('\n') {
                    formatted = format!("\n{formatted}");
                }
            }
            _ => {} // Replace, Around don't need special newline handling
        }

        formatted
    }
}

impl LanguageEditor for PythonEditor {
    fn apply_operation<'tree>(
        &self,
        node: tree_sitter::Node<'tree>,
        tree: &tree_sitter::Tree,
        operation: &EditOperation,
        source_code: &str,
    ) -> Result<crate::operations::EditResult> {
        let EditOperation { target, content } = operation;
        match (&target.position, content) {
            (None, _) => std::todo!(),
            (Some(position), Some(content)) => {
                // Format content with Python-specific rules
                let formatted_content = self.format_content_for_insertion(node, content, position);

                match position {
                    Before => self.insert_before(node, tree, source_code, &formatted_content),
                    After => self.insert_after(node, tree, source_code, &formatted_content),
                    Around => self.wrap(node, tree, source_code, &formatted_content),
                    Replace => self.replace(node, tree, source_code, &formatted_content),
                }
            }
            (Some(Replace), None) => self.delete(node, tree, source_code),
            (Some(op), None) => Err(anyhow!("Content required for {op:?}")),
        }
    }

    fn collect_errors(&self, _tree: &tree_sitter::Tree, content: &str) -> Vec<usize> {
        if let Some(err) =
            rustpython_parser::parse(content, rustpython_parser::Mode::Module, "anonymous.py").err()
        {
            let converter = LineConverter::new(content);
            vec![converter.textsize_to_line(err.offset)]
        } else {
            vec![]
        }
    }
}

struct LineConverter {
    newline_positions: Vec<usize>,
}

impl LineConverter {
    fn new(text: &str) -> Self {
        let newline_positions = std::iter::once(0)
            .chain(text.match_indices('\n').map(|(i, _)| i + 1))
            .chain(std::iter::once(text.len())) // End of file
            .collect();

        Self { newline_positions }
    }

    fn textsize_to_line(&self, offset: TextSize) -> usize {
        let byte_offset = usize::from(offset); // Safe conversion
        match self.newline_positions.binary_search(&byte_offset) {
            Ok(line) => line + 1,
            Err(line) => line,
        }
    }
}
