use crate::languages::{traits::LanguageEditor, LanguageCommon, LanguageName};

pub fn language() -> LanguageCommon {
    LanguageCommon {
        name: LanguageName::Python,
        file_extensions: &["py", "pyi"],
        language: tree_sitter_python::LANGUAGE.into(),
        editor: Box::new(PythonEditor),
        validation_query: None,
    }
}

struct PythonEditor;

impl LanguageEditor for PythonEditor {}

// struct LineConverter {
//     newline_positions: Vec<usize>,
// }

// impl LineConverter {
//     fn new(text: &str) -> Self {
//         let newline_positions = std::iter::once(0)
//             .chain(text.match_indices('\n').map(|(i, _)| i + 1))
//             .chain(std::iter::once(text.len())) // End of file
//             .collect();

//         Self { newline_positions }
//     }
//     fn textsize_to_line(&self, offset: rustpython_parser::ast::TextSize) -> usize {
//         let byte_offset = usize::from(offset); // Safe conversion
//         match self.newline_positions.binary_search(&byte_offset) {
//             Ok(line) => line + 1,
//             Err(line) => line,
//         }
//     }
// }
