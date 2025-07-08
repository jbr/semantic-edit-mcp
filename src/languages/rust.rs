use crate::editor::{Edit, EditIterator, Editor};

use super::{traits::LanguageEditor, LanguageCommon, LanguageName};
use anyhow::{anyhow, Result};
use std::{
    io::{Read, Write},
    process::{Command, Stdio},
};
use tree_sitter::{Node, Tree};

pub fn language() -> Result<LanguageCommon> {
    let language = tree_sitter_rust::LANGUAGE.into();
    let validation_query = Some(tree_sitter::Query::new(
        &language,
        include_str!("../../queries/rust/validation.scm"),
    )?);
    let editor = Box::new(RustEditor);

    Ok(LanguageCommon {
        language,
        validation_query,
        editor,
        name: LanguageName::Rust,
        file_extensions: &["rs"],
    })
}

struct RustEditor;

impl LanguageEditor for RustEditor {
    fn format_code(&self, source: &str) -> Result<String> {
        let mut child = Command::new("rustfmt")
            .args(["--emit", "stdout", "--edition", "2024"])
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

    fn build_edits<'language, 'editor>(
        &self,
        editor: &'editor Editor<'language>,
    ) -> Result<Vec<Edit<'editor, 'language>>, String> {
        let edit_iterator = EditIterator::new(editor);

        let mut edits = edit_iterator.find_edits()?;

        let mut parser = editor.language().tree_sitter_parser().unwrap();
        let content_parse = parser.parse(editor.content(), None);
        let Some(content_parse) = content_parse else {
            return Ok(edits);
        };

        for edit in &mut edits {
            handle_grouping(edit, &content_parse);
        }

        Ok(edits)
    }
}

fn handle_grouping<'language, 'editor>(
    edit: &mut Edit<'language, 'editor>,
    replacement_tree: &Tree,
) -> Option<()> {
    let root = replacement_tree.root_node();
    let mut walk = root.walk();
    let mut replacement = root.children(&mut walk);

    let edit_node = edit.node()?;
    let replacement_node =
        replacement.find(|replacement_node| replacement_node.kind() == edit_node.kind())?;

    // return if the replacement content has no preceding nodes
    find_grouped_nodes(replacement_node)?;

    let source_preceeding = find_grouped_nodes(*edit_node)?;
    edit.position_mut()
        .set_start_byte(source_preceeding.last()?.start_byte());
    edit.set_node(None);
    None
}

fn find_grouped_nodes<'a>(mut node: Node<'a>) -> Option<Vec<Node<'a>>> {
    let mut group: Option<Vec<Node<'a>>> = None;

    while let Some(prev) = node.prev_sibling() {
        if let "line_comment" | "attribute_item" = prev.kind() {
            node = prev;
            group.get_or_insert_default().push(prev);
        } else {
            break;
        }
    }

    group
}
