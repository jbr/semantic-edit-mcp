//! Targeting an edit at a **named item** instead of at anchor text.
//!
//! Textual anchors remain the general path — they work in every file, including
//! ones with no grammar. This is the narrow extension for the one failure they
//! cannot express: an insertion adjacent to an item whose leading attributes and
//! doc comments belong to it.
//!
//! Observed: to insert a test before `prose_still_reaches_the_sink_as_text`, the
//! anchor supplied was the `fn` line. The doc comment and `#[test]` above it sit
//! before that boundary, so the new function was inserted *below* them — it
//! inherited the old test's documentation and attribute, and the original lost
//! both. The compiler caught it; the edit result did not.
//!
//! An item reference names `{kind, name}`. What it resolves to is the item
//! **including its leading outer attributes and doc comments**, so `insert_before`
//! is unambiguously above the whole decorated unit and `insert_after` below it.
//! This is deliberately not a query language: one name, one kind, one side. Widen
//! it only on evidence from other repeated targeting failures.

use serde::{Deserialize, Serialize};
use std::fmt::{self, Display, Formatter};
use tree_sitter::{Node, Tree};

/// How many items an unresolved reference lists back, before saying how many more
/// there were. Enough to recognize a near-miss; not the whole file.
const MAX_LISTED: usize = 30;

/// A named item to target: `{ "kind": "function", "name": "handle_frame" }`.
///
/// `kind` is matched loosely against the grammar's node kind, so the word a
/// language's users say works across grammars that spell it differently —
/// `function` matches Rust's `function_item`, Python's `function_definition`, and
/// TypeScript's `function_declaration` / `method_definition`.
#[derive(Debug, Clone, Deserialize, Serialize, schemars::JsonSchema, Eq, PartialEq)]
pub struct ItemRef {
    /// The kind of item: `function`, `struct`, `enum`, `trait`, `class`, `type`,
    /// `const`, `mod`, … Matched against the file grammar's own node kinds, with
    /// the `_item` / `_definition` / `_declaration` suffix and `fn` / `method`
    /// spellings treated as the same word.
    pub kind: String,

    /// The item's own name, exactly as it is declared.
    pub name: String,
}

impl Display for ItemRef {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{} `{}`", self.kind, self.name)
    }
}

impl std::str::FromStr for ItemRef {
    type Err = String;

    /// The command line spelling: `--item function:handle_frame`. The wire form is
    /// the object; this exists so the same targeting is reachable from the CLI.
    fn from_str(s: &str) -> Result<Self, String> {
        let (kind, name) = s
            .split_once(':')
            .ok_or_else(|| format!("expected `kind:name`, got {s:?}"))?;
        if kind.trim().is_empty() || name.trim().is_empty() {
            return Err(format!("expected `kind:name`, got {s:?}"));
        }
        Ok(Self {
            kind: kind.trim().to_string(),
            name: name.trim().to_string(),
        })
    }
}

/// A resolved item: the byte range that an edit targets — decorations included —
/// plus what sits on either side of it, so a result can say where the edit landed
/// in terms the caller can check.
#[derive(Debug)]
pub(crate) struct ResolvedItem<'tree> {
    /// The decorations and the item itself, in source order. Passing this as the
    /// edit's node set is also what keeps language-specific grouping from
    /// re-expanding a selection that is already complete.
    pub(crate) nodes: Vec<Node<'tree>>,
    /// First byte of the first decoration (or of the item, undecorated).
    pub(crate) start: usize,
    /// Last byte of the item.
    pub(crate) end: usize,
    /// How the item is described back to the caller (`function \`alpha\``).
    pub(crate) description: String,
    /// 1-based line span of `start..end`.
    pub(crate) lines: (usize, usize),
    /// How many leading attributes / doc comments were taken in.
    pub(crate) decorations: usize,
    /// The item before and after this one in the same scope, described the same
    /// way — what makes an insertion's placement checkable.
    pub(crate) previous: Option<String>,
    pub(crate) next: Option<String>,
}

/// Locate the item `reference` names, or explain what the file does contain.
pub(crate) fn resolve<'tree>(
    tree: &'tree Tree,
    source: &str,
    reference: &ItemRef,
) -> Result<ResolvedItem<'tree>, String> {
    let mut found = Vec::new();
    let mut all = Vec::new();
    collect(tree.root_node(), source, &mut all);

    for node in &all {
        if name_of(*node, source).as_deref() == Some(reference.name.as_str())
            && kinds_match(&reference.kind, node.kind())
        {
            found.push(*node);
        }
    }

    let node = match found.len() {
        1 => found[0],
        0 => return Err(not_found(reference, &all, source)),
        n => {
            // Two items of the same kind and name in one file (a method on two
            // different impls, a function in two modules). Naming one of them is
            // not something the caller can express here, so say so rather than
            // silently taking the first — a textual anchor can disambiguate.
            let lines = found
                .iter()
                .map(|node| (line_of(source, node.start_byte())).to_string())
                .collect::<Vec<_>>()
                .join(", ");
            return Err(format!(
                "{reference} names {n} items in this file (lines {lines}); an item reference \
cannot say which. Use a text `anchor` that covers the one you mean. The file was not modified."
            ));
        }
    };

    let decorations = leading_decorations(node, source);
    let start = decorations.first().unwrap_or(&node).start_byte();
    let end = node.end_byte();
    let mut nodes = decorations.clone();
    nodes.push(node);

    Ok(ResolvedItem {
        start,
        end,
        description: describe(node, source),
        lines: (
            line_of(source, start),
            line_of(source, end.saturating_sub(1)),
        ),
        decorations: decorations.len(),
        previous: sibling_description(decorations.first().unwrap_or(&node), source, false),
        next: sibling_description(&node, source, true),
        nodes,
    })
}

/// Every named node in the tree that declares a name. Nested scopes are included,
/// so a method on an `impl` or a function in a `mod` is reachable.
fn collect<'tree>(node: Node<'tree>, source: &str, out: &mut Vec<Node<'tree>>) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.is_named() && name_of(child, source).is_some() {
            out.push(child);
        }
        collect(child, source, out);
    }
}

/// A node's declared name, from the grammar's own `name` field.
fn name_of(node: Node<'_>, source: &str) -> Option<String> {
    let name = node.child_by_field_name("name")?;
    let text = source.get(name.start_byte()..name.end_byte())?;
    (!text.trim().is_empty()).then(|| text.trim().to_string())
}

/// `function` matches `function_item`, `function_definition`, `function_declaration`
/// and `method_definition`; `mod` matches `module`. Grammars spell the same word
/// several ways, and the caller should not have to know which one this file uses.
fn kinds_match(requested: &str, node_kind: &str) -> bool {
    requested == node_kind || canonical(requested) == canonical(node_kind)
}

fn canonical(kind: &str) -> &str {
    let base = ["_item", "_definition", "_declaration", "_specifier"]
        .iter()
        .find_map(|suffix| kind.strip_suffix(suffix))
        .unwrap_or(kind);
    match base {
        "fn" | "func" | "method" | "function" => "function",
        "mod" | "module" => "mod",
        other => other,
    }
}

/// The contiguous run of attributes and comments directly above `node`, in source
/// order. A blank line ends the run: a comment separated from an item by an empty
/// line is not decorating it, and swallowing it would move a section header.
fn leading_decorations<'tree>(node: Node<'tree>, source: &str) -> Vec<Node<'tree>> {
    let mut decorations = Vec::new();
    let mut current = node;
    while let Some(previous) = current.prev_sibling() {
        if !is_decoration(previous) {
            break;
        }
        let between = source
            .get(previous.end_byte()..current.start_byte())
            .unwrap_or("");
        if between.matches('\n').count() > 1 {
            break;
        }
        decorations.push(previous);
        current = previous;
    }
    decorations.reverse();
    decorations
}

/// Comments and attributes, across grammars: Rust's `attribute_item` and
/// `line_comment`, Python's `decorator` and `comment`, the ECMAScript family's
/// `comment` and `decorator`.
fn is_decoration(node: Node<'_>) -> bool {
    let kind = node.kind();
    kind.contains("comment") || kind.contains("attribute") || kind.contains("decorator")
}

/// `function \`alpha\`` — the kind as the caller would say it, plus the name.
fn describe(node: Node<'_>, source: &str) -> String {
    match name_of(node, source) {
        Some(name) => format!("{} `{name}`", canonical(node.kind())),
        None => canonical(node.kind()).to_string(),
    }
}

/// The named item immediately before or after `node` among its siblings, skipping
/// decorations (which belong to whatever they decorate, not to the gap).
fn sibling_description(node: &Node<'_>, source: &str, forward: bool) -> Option<String> {
    let mut current = *node;
    loop {
        let sibling = if forward {
            current.next_sibling()
        } else {
            current.prev_sibling()
        }?;
        if !is_decoration(sibling) && sibling.is_named() {
            return Some(describe(sibling, source));
        }
        current = sibling;
    }
}

/// 1-based line containing `byte`.
fn line_of(source: &str, byte: usize) -> usize {
    source[..byte.min(source.len())]
        .bytes()
        .filter(|b| *b == b'\n')
        .count()
        + 1
}

/// What an unresolved reference gets back: what the file actually declares. A bare
/// "not found" costs a round trip to learn whether the name, the kind, or the file
/// was wrong.
fn not_found(reference: &ItemRef, all: &[Node<'_>], source: &str) -> String {
    let same_name: Vec<String> = all
        .iter()
        .filter(|node| name_of(**node, source).as_deref() == Some(reference.name.as_str()))
        .map(|node| describe(*node, source))
        .collect();
    if !same_name.is_empty() {
        return format!(
            "No {reference} in this file, but that name is declared as: {}. \
The file was not modified.",
            same_name.join(", ")
        );
    }

    let matching_kind: Vec<String> = all
        .iter()
        .filter(|node| kinds_match(&reference.kind, node.kind()))
        .map(|node| describe(*node, source))
        .collect();
    let (listed, label) = if matching_kind.is_empty() {
        (
            all.iter()
                .map(|node| describe(*node, source))
                .collect::<Vec<_>>(),
            "named items",
        )
    } else {
        (matching_kind, "items of that kind")
    };

    if listed.is_empty() {
        return format!(
            "No {reference} in this file, which declares no named items at all — \
use a text `anchor`. The file was not modified."
        );
    }

    let shown = listed.len().min(MAX_LISTED);
    let more = if listed.len() > shown {
        format!(" (and {} more)", listed.len() - shown)
    } else {
        String::new()
    };
    format!(
        "No {reference} in this file. Its {label}: {}{more}. The file was not modified.",
        listed[..shown].join(", ")
    )
}

#[cfg(test)]
mod test;
