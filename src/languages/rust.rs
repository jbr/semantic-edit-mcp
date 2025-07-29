use crate::editor::{Edit, EditIterator, Editor};

use super::{LanguageCommon, LanguageName, traits::LanguageEditor};
use anyhow::{Result, anyhow};
use std::{
    io::{Read, Write},
    path::Path,
    process::{Command, Stdio},
};
use tree_sitter::{Node, Query};

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
                let root = content_parse.root_node();
                let mut walk = root.walk();
                let replacement = root.children(&mut walk).collect::<Vec<_>>();

                if let Some("line_comment") = replacement.last().map(|node| node.kind())
                    && let Some("line_comment") = edit
                        .nodes()
                        .and_then(|nodes| nodes.last().map(|node| node.kind()))
                    && !editor.content().ends_with('\n')
                {
                    edit.content_mut().to_mut().push('\n');
                } else {
                    handle_grouping(edit, &replacement);
                }
            }
        }

        Ok(edits)
    }
}

fn handle_grouping<'editor, 'language>(
    edit: &mut Edit<'editor, 'language>,
    replacement: &[Node<'_>],
) -> Option<()> {
    let replacement_has_preceding = replacement.iter().any(|node| is_preceding_item(*node));
    let replacement_has_primary = replacement.iter().any(|node| !is_preceding_item(*node));

    // Keep expanding until no more expansions are needed
    while let Some(expansion_type) = determine_expansion_type(
        edit.nodes()?,
        replacement_has_preceding,
        replacement_has_primary,
    ) {
        let expanded_nodes: Vec<Node<'editor>> = match expansion_type {
            ExpansionType::BackwardsToMatchPattern => {
                let replacement_pattern = get_preceding_pattern(replacement);
                expand_backwards_to_match_pattern(edit.nodes()?, &replacement_pattern)?
            }
            ExpansionType::ForwardToIncludePrimary => {
                expand_forward_to_include_primary(edit.nodes()?)?
            }
        };

        apply_expansion(edit, expanded_nodes, expansion_type);
    }

    Some(())
}

#[derive(Debug)]
enum ExpansionType {
    BackwardsToMatchPattern,
    ForwardToIncludePrimary,
}

fn determine_expansion_type<'editor>(
    edit_nodes: &[Node<'editor>],
    replacement_has_preceding: bool,
    replacement_has_primary: bool,
) -> Option<ExpansionType> {
    let selection_has_preceding = edit_nodes.iter().any(|node| is_preceding_item(*node));
    let selection_has_primary = edit_nodes.iter().any(|node| !is_preceding_item(*node));
    let selection_only_preceding = selection_has_preceding && !selection_has_primary;

    if replacement_has_preceding && replacement_has_primary && !selection_has_preceding {
        // Risk of duplication: expand backwards to include matching preceding items
        Some(ExpansionType::BackwardsToMatchPattern)
    } else if selection_only_preceding && replacement_has_primary {
        // Selected only comments/attributes but replacing with full logical unit
        Some(ExpansionType::ForwardToIncludePrimary)
    } else {
        // No expansion needed
        None
    }
}

fn get_preceding_pattern<'a>(replacement: &[Node<'a>]) -> Vec<&'a str> {
    replacement
        .iter()
        .take_while(|node| is_preceding_item(**node))
        .map(|node| node.kind())
        .collect()
}

fn expand_backwards_to_match_pattern<'a>(
    edit_nodes: &[Node<'a>],
    replacement_pattern: &[&str],
) -> Option<Vec<Node<'a>>> {
    if replacement_pattern.is_empty() {
        return None;
    }

    // Find first primary node in selection
    let first_primary = edit_nodes.iter().find(|node| !is_preceding_item(**node))?;

    // Walk backwards from first primary node to collect matching preceding items
    let mut preceding_items = Vec::new();
    let mut current = *first_primary;
    let mut pattern_index = replacement_pattern.len();

    // Walk backwards, matching the replacement pattern in reverse
    while let Some(prev) = current.prev_sibling() {
        if pattern_index > 0 && is_preceding_item(prev) {
            let expected_kind = replacement_pattern[pattern_index - 1];
            if prev.kind() == expected_kind {
                preceding_items.insert(0, prev);
                pattern_index -= 1;
                current = prev;
                continue;
            }
        }
        break;
    }

    // Only return Some if we actually found preceding items to add
    if preceding_items.is_empty() {
        return None;
    }

    // Build the expanded selection
    let mut expanded = preceding_items;
    expanded.extend_from_slice(edit_nodes);
    Some(expanded)
}

fn expand_forward_to_include_primary<'a>(edit_nodes: &[Node<'a>]) -> Option<Vec<Node<'a>>> {
    let last_node = edit_nodes.last()?;
    let mut expanded = edit_nodes.to_vec();
    let mut current = *last_node;

    // Walk forward until we find a primary item
    while let Some(next) = current.next_sibling() {
        expanded.push(next);
        if !is_preceding_item(next) {
            break; // Found the primary item
        }
        current = next;
    }

    Some(expanded)
}

fn apply_expansion<'editor, 'language>(
    edit: &mut Edit<'editor, 'language>,
    expanded_nodes: Vec<Node<'editor>>,
    expansion_type: ExpansionType,
) {
    let position = edit.position_mut();

    if let (Some(first), Some(last)) = (expanded_nodes.first(), expanded_nodes.last()) {
        position.set_start_byte(first.start_byte());
        if position.end_byte.is_some() {
            position.end_byte = Some(last.end_byte());
        }
    }

    let annotation = match expansion_type {
        ExpansionType::BackwardsToMatchPattern => "rust: expanded backwards to match pattern",
        ExpansionType::ForwardToIncludePrimary => "rust: expanded forward to include primary",
    };

    edit.set_annotation(annotation).set_nodes(expanded_nodes);
}

fn is_preceding_item(node: Node<'_>) -> bool {
    matches!(
        node.kind(),
        "line_comment" | "block_comment" | "attribute_item"
    )
}
