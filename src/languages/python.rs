use crate::{
    editor::{Edit, EditIterator, Editor},
    indentation::Indentation,
    languages::{traits::LanguageEditor, LanguageCommon, LanguageName},
};
use anyhow::{anyhow, Result};
use std::{
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
};
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

impl LanguageEditor for PythonEditor {
    fn build_edits<'language, 'editor>(
        &self,
        editor: &'editor Editor<'language>,
    ) -> Result<Vec<Edit<'editor, 'language>>, String> {
        let edit_iterator = EditIterator::new(editor);

        let mut edits = edit_iterator.find_edits()?;

        log::trace!("🐍 PYTHON EDITOR CALLED with {} edits", edits.len());

        for edit in &mut edits {
            Self::adjust_indentation(edit);
        }

        Ok(edits)
    }

    fn format_code(&self, source: &str, _file_path: &Path) -> Result<String> {
        let mut child = Command::new("ruff")
            .args(["format", "-"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;

        if let Some(mut stdin) = child.stdin.take() {
            stdin.write_all(source.as_bytes())?;
            drop(stdin);
        }

        let mut stdout = String::new();
        if let Some(mut out) = child.stdout.take() {
            out.read_to_string(&mut stdout)?;
        }

        let mut stderr = String::new();
        if let Some(mut err) = child.stderr.take() {
            err.read_to_string(&mut stderr)?;
        }

        if child.wait()?.success() {
            Ok(stdout)
        } else {
            Err(anyhow!(stderr))
        }
    }
}

impl PythonEditor {
    fn adjust_indentation<'language, 'editor>(edit: &mut Edit<'editor, 'language>) {
        let source_code = edit.source_code();
        let start_byte = edit.start_byte();
        let content = edit.content_mut();

        let line_start = source_code[..start_byte]
            .rfind('\n')
            .map(|pos| pos + 1) // +1 to get position after the newline
            .unwrap_or(0); // If no newline found, start of file

        let line_end = source_code[start_byte..]
            .find(|x: char| !x.is_whitespace() || x == '\n')
            .map(|newline| start_byte + newline)
            .unwrap_or(source_code.len());

        // Detect the file's indentation style
        let file_indentation =
            Indentation::determine(source_code).unwrap_or(Indentation::Spaces(4));

        let target_indentation_count =
            file_indentation.unit_count(&source_code[line_start..line_end]);

        file_indentation.reindent(target_indentation_count, content);

        edit.set_start_byte(line_start);
    }
}
