mod context_validator;
pub use context_validator::{ContextValidator, format_violations};

use std::fmt::{self, Display, Formatter};

/// Why a language's [check](crate::languages::traits::LanguageEditor::check)
/// rejects a source: the checker's own message, and where it points, when it
/// says.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Diagnostic {
    /// The checker's message, in its own words.
    pub message: String,
    /// The line it points at, counting from 1.
    pub line: Option<usize>,
    /// The column on that line it points at, in characters, counting from 1.
    pub column: Option<usize>,
}

impl Display for Diagnostic {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match (self.line, self.column) {
            (Some(line), Some(column)) => write!(f, "line {line}, column {column}: ")?,
            (Some(line), None) => write!(f, "line {line}: ")?,
            _ => {}
        }
        f.write_str(&self.message)
    }
}
