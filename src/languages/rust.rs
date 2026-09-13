use crate::editor::{Edit, EditIterator, Editor};
use crate::selector::Operation;

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
    fn format_code(&self, source: &str, file_path: &Path) -> Result<String> {
        // let source = syn::parse_file(source)?;
        // Ok(prettyplease::unparse(&source))

        let edition = rust_edition(file_path);
        let mut child = Command::new("rustfmt")
            .args(["--emit", "stdout", "--edition", edition])
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
        let mut parser = editor
            .language()
            .tree_sitter_parser()
            .map_err(|e| e.to_string())?;
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
                    handle_grouping(edit, &replacement, editor.content());
                }
            }
        }

        Ok(edits)
    }
}

fn handle_grouping<'editor, 'language>(
    edit: &mut Edit<'editor, 'language>,
    replacement: &[Node<'_>],
    replacement_src: &str,
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
            ExpansionType::BackwardsToMatchPattern => reconcile_backward(
                edit.nodes()?,
                replacement,
                edit.source_code(),
                replacement_src,
            )?,
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

/// Reconcile the target item's existing leading decorations (comments / attributes)
/// against the decorations the replacement supplies, returning the node set that
/// should be replaced — the target decorations *from the topmost shared one
/// downward*, plus the originally-selected nodes — or `None` when nothing should be
/// consumed.
///
/// The rule: find the topmost target decoration whose identity also appears in the
/// replacement (the *anchor*). From the anchor down, the replacement is
/// authoritative, so those decorations are consumed and replaced; decorations
/// *above* the anchor are left untouched. If the replacement shares no decoration
/// with the target, consume nothing — the existing decorations are preserved and
/// the replacement's are simply added. This is why
/// `#[derive(Clone)] pub struct …` over `#[derive(Debug)] /// doc pub struct …`
/// replaces the derive but, when the shared `derive` is the topmost match, keeps
/// whatever sits above it.
fn reconcile_backward<'a>(
    edit_nodes: &[Node<'a>],
    replacement: &[Node<'_>],
    target_src: &str,
    replacement_src: &str,
) -> Option<Vec<Node<'a>>> {
    let first = edit_nodes.first()?;

    // Target's contiguous leading decorations: walk siblings backward (nearest
    // first), then flip to top-to-bottom source order.
    let mut decorations = Vec::new();
    let mut current = *first;
    while let Some(prev) = current.prev_sibling() {
        if is_preceding_item(prev) {
            decorations.push(prev);
            current = prev;
        } else {
            break;
        }
    }
    if decorations.is_empty() {
        return None;
    }
    decorations.reverse();

    // Identities the replacement re-states.
    let replacement_keys: Vec<String> = replacement
        .iter()
        .take_while(|node| is_preceding_item(**node))
        .map(|node| decoration_key(*node, replacement_src))
        .collect();

    // Topmost target decoration the replacement also has — the anchor.
    let anchor = decorations
        .iter()
        .position(|node| replacement_keys.contains(&decoration_key(*node, target_src)))?;

    let mut expanded: Vec<Node<'a>> = decorations[anchor..].to_vec();
    expanded.extend_from_slice(edit_nodes);
    Some(expanded)
}

/// A stable identity for a leading decoration, used to detect which decorations the
/// replacement re-states. Attributes are keyed by their path (`derive`, `serde`, …)
/// so `#[derive(Debug)]` and `#[derive(Debug, Clone)]` are the same slot; any doc
/// comment is a single `doc` slot (so changing its text still matches); ordinary
/// comments are keyed by exact text.
fn decoration_key(node: Node<'_>, source: &str) -> String {
    let text = source
        .get(node.start_byte()..node.end_byte())
        .unwrap_or("")
        .trim_start();
    match node.kind() {
        "attribute_item" => format!("attr:{}", attr_path(text)),
        "line_comment" | "block_comment"
            if text.starts_with("///")
                || text.starts_with("//!")
                || text.starts_with("/**")
                || text.starts_with("/*!") =>
        {
            "doc".to_string()
        }
        _ => format!("text:{text}"),
    }
}

/// Extract an attribute's leading path from its source text, e.g. `derive` from
/// `#[derive(Debug)]` or `tokio::test` from `#[tokio::test]`.
fn attr_path(text: &str) -> &str {
    let rest = text
        .trim_start_matches('#')
        .trim_start_matches('!')
        .trim_start()
        .strip_prefix('[')
        .unwrap_or(text)
        .trim_start();
    let end = rest
        .find(|c: char| matches!(c, '(' | '[' | '{' | '=' | ']') || c.is_whitespace())
        .unwrap_or(rest.len());
    rest[..end].trim()
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
    // An `insert_after` edit's position is an end-anchored insertion point
    // (`start_byte == end_byte == node.end`, see `Edit::insert_after`). Expanding
    // the selection to group preceding doc-comments/attributes must keep that point
    // at the *end* of the group (after the primary's `}`), not drag it back to the
    // group's start — otherwise "insert after the function" silently inserts before
    // its doc-comment. `insert_before` and `replace` anchor on the group's start.
    let operation = edit.operation();
    let position = edit.position_mut();

    if let (Some(first), Some(last)) = (expanded_nodes.first(), expanded_nodes.last()) {
        if matches!(operation, Operation::InsertAfter) {
            position.set_start_byte(last.end_byte());
        } else {
            position.set_start_byte(first.start_byte());
            if position.end_byte.is_some() {
                position.end_byte = Some(last.end_byte());
            }
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

/// Editions `rustfmt` understands; anything else (a typo, a future edition) falls
/// back to [`DEFAULT_EDITION`] rather than making rustfmt error.
const KNOWN_EDITIONS: &[&str] = &["2015", "2018", "2021", "2024"];

/// Edition assumed when no `Cargo.toml` governs the file (e.g. a standalone
/// snippet). The newest edition is the most permissive about modern syntax.
const DEFAULT_EDITION: &str = "2024";

/// Resolve the Rust edition that applies to `file_path` for formatting, always
/// returning one of [`KNOWN_EDITIONS`]. Walks up to the nearest `Cargo.toml`,
/// honoring workspace inheritance and Cargo's "no `edition` key means 2015" rule,
/// and falls back to [`DEFAULT_EDITION`] when nothing declares one.
fn rust_edition(file_path: &Path) -> &'static str {
    resolve_edition(file_path)
        .as_deref()
        .and_then(clamp_edition)
        .unwrap_or(DEFAULT_EDITION)
}

fn clamp_edition(edition: &str) -> Option<&'static str> {
    KNOWN_EDITIONS
        .iter()
        .copied()
        .find(|known| *known == edition)
}

/// What a single manifest tells us about the edition.
#[derive(Debug, PartialEq, Eq)]
enum ManifestEdition {
    /// Determined by this manifest (explicit, defaulted to 2015, or inheritance
    /// satisfied by this same file's `[workspace.package]`).
    Edition(String),
    /// `[package] edition.workspace = true` but no `[workspace.package].edition`
    /// here — the value lives in a workspace manifest further up.
    InheritFromAbove,
    /// No `[package]` table (a virtual workspace manifest); keep walking.
    Unknown,
}

fn resolve_edition(file_path: &Path) -> Option<String> {
    let mut dir = file_path.parent();
    let mut inheriting = false;

    while let Some(current) = dir {
        if let Ok(text) = std::fs::read_to_string(current.join("Cargo.toml")) {
            if inheriting {
                if let Some(edition) = workspace_package_edition(&text) {
                    return Some(edition);
                }
            } else {
                match edition_in_manifest(&text) {
                    ManifestEdition::Edition(edition) => return Some(edition),
                    ManifestEdition::InheritFromAbove => inheriting = true,
                    ManifestEdition::Unknown => {}
                }
            }
        }
        dir = current.parent();
    }

    None
}

/// Extract the `[workspace.package].edition` from a manifest, if present.
fn workspace_package_edition(text: &str) -> Option<String> {
    let dom = taplo::parser::parse(text).into_dom();
    dom.get("workspace")
        .get("package")
        .get("edition")
        .as_str()
        .map(|s| s.value().to_string())
}

/// Interpret a single manifest's `[package]` edition, applying Cargo's rules.
fn edition_in_manifest(text: &str) -> ManifestEdition {
    let dom = taplo::parser::parse(text).into_dom();
    let package = dom.get("package");

    if package.as_table().is_none() {
        return ManifestEdition::Unknown;
    }

    let edition = package.get("edition");

    if let Some(value) = edition.as_str() {
        return ManifestEdition::Edition(value.value().to_string());
    }

    // `edition.workspace = true` / `edition = { workspace = true }`
    if edition
        .get("workspace")
        .as_bool()
        .is_some_and(|b| b.value())
    {
        return match workspace_package_edition(text) {
            Some(edition) => ManifestEdition::Edition(edition),
            None => ManifestEdition::InheritFromAbove,
        };
    }

    // `[package]` present without an `edition` key → Cargo defaults to 2015.
    ManifestEdition::Edition("2015".to_string())
}

#[cfg(test)]
mod edition_tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn explicit_edition() {
        assert_eq!(
            edition_in_manifest("[package]\nname = \"x\"\nedition = \"2018\"\n"),
            ManifestEdition::Edition("2018".into())
        );
    }

    #[test]
    fn missing_edition_defaults_to_2015() {
        assert_eq!(
            edition_in_manifest("[package]\nname = \"x\"\n"),
            ManifestEdition::Edition("2015".into())
        );
    }

    #[test]
    fn workspace_inheritance_satisfied_in_same_file() {
        let manifest = "[package]\nname = \"x\"\nedition.workspace = true\n\n\
                        [workspace.package]\nedition = \"2021\"\n";
        assert_eq!(
            edition_in_manifest(manifest),
            ManifestEdition::Edition("2021".into())
        );
    }

    #[test]
    fn workspace_inheritance_deferred_upwards() {
        assert_eq!(
            edition_in_manifest("[package]\nname = \"x\"\nedition = { workspace = true }\n"),
            ManifestEdition::InheritFromAbove
        );
    }

    #[test]
    fn virtual_manifest_is_unknown() {
        assert_eq!(
            edition_in_manifest("[workspace]\nmembers = []\n"),
            ManifestEdition::Unknown
        );
    }

    #[test]
    fn clamp_rejects_unknown_editions() {
        assert_eq!(clamp_edition("2021"), Some("2021"));
        assert_eq!(clamp_edition("2030"), None);
        assert_eq!(clamp_edition(""), None);
    }

    #[test]
    fn walk_resolves_real_manifest() {
        // This crate's Cargo.toml declares edition 2024; a file inside it resolves
        // to that (and this is why the in-repo snapshot fixtures stay on 2024).
        let in_repo = Path::new(env!("CARGO_MANIFEST_DIR")).join("src/languages/rust.rs");
        assert_eq!(rust_edition(&in_repo), "2024");
    }

    #[test]
    fn walk_falls_back_when_no_manifest() {
        // No Cargo.toml anywhere above this path → default, no panic.
        assert_eq!(
            rust_edition(Path::new("/semantic-edit-nonexistent/a/b/file.rs")),
            DEFAULT_EDITION
        );
    }
}
