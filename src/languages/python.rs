use crate::{
    editor::{Edit, EditIterator, Editor},
    indentation::Indentation,
    languages::{LanguageCommon, LanguageName, traits::LanguageEditor},
};
use anyhow::{Result, anyhow};
use std::{
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
};
use tree_sitter::Query;
pub fn language() -> LanguageCommon {
    let language = tree_sitter_python::LANGUAGE.into();
    let query = Query::new(
        &language,
        include_str!("../../queries/python/validation.scm"),
    )
    .unwrap();

    LanguageCommon {
        name: LanguageName::Python,
        file_extensions: &["py", "pyi"],
        language,
        editor: Box::new(PythonEditor),
        validation_query: Some(query),
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

        let additional_edits = edits
            .iter()
            .filter_map(|edit| {
                edit.nodes()
                    .and_then(|nodes| {
                        nodes.iter().find_map(|node| {
                            node.children(&mut node.walk())
                                .find(|node| node.kind() == "block")
                        })
                    })
                    .map(|block| {
                        [
                            edit.clone()
                                .with_nodes(vec![block])
                                .with_start_byte(block.start_byte())
                                .with_annotation("python: inside block"),
                            edit.clone()
                                .with_nodes(vec![block])
                                .with_start_byte(block.start_byte())
                                .with_content(format!("{}\n", edit.content()))
                                .with_annotation("python: inside block with newline"),
                        ]
                    })
            })
            .flatten()
            .collect::<Vec<_>>();
        edits.extend(additional_edits);

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
        let mut start_byte = edit.start_byte();

        let line_start = find_line_start(source_code, start_byte);

        let line_end = source_code[start_byte..]
            .find(|x: char| !x.is_whitespace() || x == '\n')
            .map(|newline| start_byte + newline)
            .unwrap_or(source_code.len());

        // Detect the file's indentation style
        let file_indentation =
            Indentation::determine(source_code).unwrap_or(Indentation::Spaces(4));

        let reference_region = if let Some(nodes) = edit.nodes()
            && let Some(first) = nodes.first()
            && let Some(last) = nodes.last()
        {
            let line_start = find_line_start(source_code, first.start_byte());

            &source_code[line_start..last.end_byte()]
        } else {
            &source_code[line_start..line_end]
        };

        let target_indentation_count = file_indentation.minimum(reference_region);

        if source_code[line_start..start_byte].trim().is_empty() {
            start_byte = line_start;
        }
        file_indentation.reindent(
            target_indentation_count,
            edit.content_mut(),
            start_byte == line_start,
        );

        edit.set_start_byte(start_byte);
    }
}

fn find_line_start(source_code: &str, start_byte: usize) -> usize {
    source_code[..start_byte]
        .rfind('\n')
        .map(|pos| pos + 1) // +1 to get position after the newline
        .unwrap_or(0) // If no newline found, start of file
}
