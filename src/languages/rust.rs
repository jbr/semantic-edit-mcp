use crate::editor::{Edit, EditIterator, Editor};

use super::{traits::LanguageEditor, LanguageCommon, LanguageName};
use anyhow::{anyhow, Result};
use std::{
    collections::VecDeque,
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
};
use tree_sitter::{Node, Query, Tree};

pub fn language() -> LanguageCommon {
    let language = tree_sitter_rust::LANGUAGE.into();
    let query = Query::new(&language, include_str!("../../queries/rust/validation.scm")).unwrap();
    LanguageCommon {
        language,
        validation_query: Some(query),
        editor: Box::new(RustEditor),
        name: LanguageName::Rust,
        file_extensions: &["rs"],
    }
}

struct RustEditor;

impl LanguageEditor for RustEditor {
    fn format_code(&self, source: &str, _file_path: &Path) -> Result<String> {
        // let source = syn::parse_file(source)?;
        // Ok(prettyplease::unparse(&source))

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
        if let Some(content_parse) = content_parse {
            for edit in &mut edits {
                handle_grouping(edit, &content_parse);
            }
        }

        Ok(edits)
    }
}

fn handle_grouping<'language, 'editor>(
    edit: &mut Edit<'language, 'editor>,
    content_parse: &Tree,
) -> Option<()> {
    let root = content_parse.root_node();
    let mut walk = root.walk();
    let mut replacement = root.children(&mut walk);

    let edit_node = edit.node()?;
    let replacement_node =
        replacement.find(|replacement_node| replacement_node.kind() == edit_node.kind())?;

    // return if the replacement content has no grouped nodes
    find_grouped_nodes(replacement_node)?;

    let source_preceeding = find_grouped_nodes(*edit_node)?;
    let position = edit.position_mut();
    position.set_start_byte(source_preceeding.first()?.start_byte());
    if position.end_byte.is_some() {
        position.end_byte = Some(source_preceeding.last()?.end_byte());
    }

    edit.set_annotation("grouped").take_node();
    None
}

fn is_preceeding_item(node: Node<'_>) -> bool {
    matches!(node.kind(), "line_comment" | "attribute_item")
}

fn find_grouped_nodes<'a>(start_node: Node<'a>) -> Option<Vec<Node<'a>>> {
    let mut group: VecDeque<Node<'a>> = VecDeque::new();
    group.push_front(start_node);

    let mut node = start_node;

    while let Some(prev) = node.prev_sibling() {
        if is_preceeding_item(prev) {
            node = prev;
            group.push_front(prev);
        } else {
            break;
        }
    }

    let mut node = start_node;

    if is_preceeding_item(start_node) {
        while let Some(next) = node.next_sibling() {
            group.push_back(next);
            if is_preceeding_item(next) {
                node = next;
            } else {
                break;
            }
        }
    }

    (group.len() > 1).then(|| group.into())
}
