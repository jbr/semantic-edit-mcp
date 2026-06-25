use crate::{
    editor::{Edit, EditIterator, Editor},
    indentation::Indentation,
    languages::LanguageEditor,
};
use anyhow::{Result, anyhow};
use std::{
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
};

/// Offer leading/trailing comma variants of each candidate so an insert into a
/// comma-separated list (object literal, array, call/param list, enum, JSON
/// members) can pick up the separator. `insert_after` needs the comma *leading*,
/// `insert_before` *trailing*; the wrong-side variant just produces a syntax error
/// and is rejected. It is inert where commas don't separate members — statement
/// contexts (`;`/newline) and JSX children both validate via the comma-free base
/// candidate, which is tried first.
pub(super) fn with_comma_variants<'editor, 'language>(
    mut edits: Vec<Edit<'editor, 'language>>,
) -> Vec<Edit<'editor, 'language>> {
    let mut variants = vec![];
    for edit in &edits {
        let content = edit.content();
        if !content.ends_with(',') {
            variants.push(
                edit.clone()
                    .with_content(format!("{content},"))
                    .with_annotation("trailing comma"),
            );
        }
        if !content.starts_with(',') {
            variants.push(
                edit.clone()
                    .with_content(format!(",{content}"))
                    .with_annotation("leading comma"),
            );
        }
    }
    edits.extend(variants);
    edits
}

pub(super) struct EcmaEditor;
impl LanguageEditor for EcmaEditor {
    fn build_edits<'language, 'editor>(
        &self,
        editor: &'editor Editor<'language>,
    ) -> Result<Vec<Edit<'editor, 'language>>, String> {
        Ok(with_comma_variants(EditIterator::new(editor).find_edits()?))
    }

    fn format_code(&self, source: &str, file_path: &Path) -> Result<String> {
        let mut command = Command::new("biome");
        command
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .arg("format")
            .arg(format!("--stdin-file-path={}", file_path.display()))
            .arg("--diagnostic-level=error");

        match Indentation::determine(source).unwrap_or(Indentation::Spaces(2)) {
            Indentation::Spaces(spaces) => command
                .arg("--indent-style=space")
                .arg(format!("--indent-width={spaces}")),

            Indentation::Tabs => command.arg("--indent-style=tab"),
        };

        let mut child = command.spawn()?;

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
