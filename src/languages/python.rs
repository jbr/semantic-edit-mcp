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

        // `insert_after` a whole statement lands at the end of the statement's
        // line, so the naive insertion glues the new content onto that line. When
        // the glued result happens to be valid Python (e.g. `a = b` + `c = d`
        // becoming the chained `a = bc = d`) it is wrongly accepted. In that case
        // indent the content as a fresh line and prefix a newline so it becomes a
        // sibling statement instead of being appended.
        if insert_after_statement(edit, source_code, start_byte) {
            file_indentation.reindent(target_indentation_count, edit.content_mut(), true);
            edit.content_mut().to_mut().insert(0, '\n');
            edit.set_start_byte(start_byte);
            return;
        }

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

/// Detect an `insert_after` whose insertion point is the end of a complete
/// statement — i.e. an insert (no `end_byte`) positioned exactly at the end of a
/// node whose parent is a statement container (`block`/`module`), with only
/// whitespace remaining on the line. This is the case that would otherwise glue a
/// new statement onto the anchor's line.
///
/// Keying on the node's *role* (statement vs. sub-expression) and on being at the
/// node's *end* is what keeps this from firing on `insert_before` (which sits at a
/// node's start) or on same-line appends like `insert_after 'def f(self'` → `,
/// arg` (whose anchor resolves inside the parameter list, not a statement
/// container).
fn insert_after_statement(edit: &Edit<'_, '_>, source: &str, start_byte: usize) -> bool {
    if edit.position().end_byte.is_some() {
        return false; // a replace, not an insert
    }

    // Only when the rest of the anchor's line is blank — i.e. we really are at the
    // line's end, not before a `;`-separated follow-on statement.
    let line_tail = source[start_byte..].split('\n').next().unwrap_or("");
    if !line_tail.trim().is_empty() {
        return false;
    }

    // The captured node is often a sub-expression (e.g. the `identifier` in an
    // assignment), so climb the ancestors that also end exactly at the insertion
    // point. If one of them is a direct child of a statement container, the
    // insertion point is a statement boundary.
    let Some(node) = edit.nodes().and_then(|nodes| nodes.last()) else {
        return false;
    };

    let mut current = *node;
    while current.end_byte() == start_byte {
        match current.parent() {
            Some(parent) => {
                if matches!(parent.kind(), "block" | "module") {
                    return true;
                }
                current = parent;
            }
            None => break,
        }
    }
    false
}

fn find_line_start(source_code: &str, start_byte: usize) -> usize {
    source_code[..start_byte]
        .rfind('\n')
        .map(|pos| pos + 1) // +1 to get position after the newline
        .unwrap_or(0) // If no newline found, start of file
}
