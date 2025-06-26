use super::{utils::parse_node_types_json, LanguageCommon};
use crate::languages::{traits::LanguageEditor, LanguageName};
use anyhow::Result;
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

impl PythonEditor {
    pub fn new() -> Self {
        Self
    }
}

impl Default for PythonEditor {
    fn default() -> Self {
        Self::new()
    }
}

impl LanguageEditor for PythonEditor {
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
