use super::*;
use crate::languages::{LanguageName, LanguageRegistry};

fn parse(source: &str) -> Tree {
    let registry = LanguageRegistry::new().unwrap();
    let language = registry.get_language(LanguageName::Rust);
    language
        .tree_sitter_parser()
        .unwrap()
        .parse(source, None)
        .unwrap()
}

const SOURCE: &str = "\
fn alpha() {}

/// Prose still streams as before, and is typed as prose.
#[test]
fn prose_still_reaches_the_sink_as_text() {
    // ...
}

fn omega() {}
";

fn item(kind: &str, name: &str) -> ItemRef {
    ItemRef {
        kind: kind.into(),
        name: name.into(),
    }
}

/// The resolved range starts at the doc comment, not at the `fn` — which is the
/// whole point: an insertion before this range cannot land between the item and
/// the attributes that belong to it.
#[test]
fn an_item_resolves_together_with_its_doc_comment_and_attributes() {
    let tree = parse(SOURCE);
    let resolved = resolve(
        &tree,
        SOURCE,
        &item("function", "prose_still_reaches_the_sink_as_text"),
    )
    .unwrap();
    assert_eq!(resolved.decorations, 2);
    assert!(
        SOURCE[resolved.start..].starts_with("/// Prose still streams"),
        "{:?}",
        &SOURCE[resolved.start..resolved.start + 20]
    );
    assert!(SOURCE[..resolved.end].ends_with('}'));
    assert_eq!(
        resolved.description,
        "function `prose_still_reaches_the_sink_as_text`"
    );
    assert_eq!(resolved.lines, (3, 7));
}

/// The neighbors are what make a placement checkable from the result alone.
#[test]
fn the_neighbors_on_each_side_are_reported() {
    let tree = parse(SOURCE);
    let resolved = resolve(
        &tree,
        SOURCE,
        &item("function", "prose_still_reaches_the_sink_as_text"),
    )
    .unwrap();
    assert_eq!(resolved.previous.as_deref(), Some("function `alpha`"));
    assert_eq!(resolved.next.as_deref(), Some("function `omega`"));
}

/// A blank line ends the decoration run: a comment separated from an item by an
/// empty line is a section header, not documentation, and swallowing it would move
/// text the caller never named.
#[test]
fn a_blank_line_ends_the_decoration_run() {
    let source = "// section header\n\n/// docs\nfn alpha() {}\n";
    let tree = parse(source);
    let resolved = resolve(&tree, source, &item("function", "alpha")).unwrap();
    assert_eq!(resolved.decorations, 1);
    assert!(source[resolved.start..].starts_with("/// docs"));
}

/// `fn`, `function` and the grammar's own `function_item` are the same word.
#[test]
fn kind_spellings_are_reconciled() {
    for kind in ["function", "fn", "function_item"] {
        let tree = parse(SOURCE);
        assert!(
            resolve(&tree, SOURCE, &item(kind, "alpha")).is_ok(),
            "{kind}"
        );
    }
    let tree = parse(SOURCE);
    assert!(resolve(&tree, SOURCE, &item("struct", "alpha")).is_err());
}

/// A miss says what the file does declare — a bare "not found" costs a round trip
/// to learn whether the name, the kind, or the file was wrong.
#[test]
fn an_unresolved_reference_lists_what_exists() {
    let tree = parse(SOURCE);
    let error = resolve(&tree, SOURCE, &item("function", "nope")).unwrap_err();
    assert!(error.contains("function `alpha`"), "{error}");
    assert!(error.contains("was not modified"), "{error}");

    let error = resolve(&tree, SOURCE, &item("struct", "alpha")).unwrap_err();
    assert!(
        error.contains("that name is declared as: function `alpha`"),
        "{error}"
    );
}

/// Two items of one kind and name cannot be told apart by a reference, so the
/// reference refuses rather than silently taking the first.
#[test]
fn an_ambiguous_reference_refuses() {
    let source = "mod a {\n    fn dup() {}\n}\nmod b {\n    fn dup() {}\n}\n";
    let tree = parse(source);
    let error = resolve(&tree, source, &item("function", "dup")).unwrap_err();
    assert!(error.contains("2 items"), "{error}");
    assert!(error.contains("anchor"), "{error}");
}

/// Nested scopes are reachable, so a method on an impl can be targeted.
#[test]
fn items_inside_a_scope_are_reachable() {
    let source = "struct S;\nimpl S {\n    /// docs\n    fn method(&self) {}\n}\n";
    let tree = parse(source);
    let resolved = resolve(&tree, source, &item("function", "method")).unwrap();
    assert_eq!(resolved.decorations, 1);
    assert_eq!(resolved.previous, None, "first item in the impl body");
}

#[test]
fn the_cli_spelling_is_kind_colon_name() {
    assert_eq!(
        "function:alpha".parse::<ItemRef>().unwrap(),
        item("function", "alpha")
    );
    assert!("alpha".parse::<ItemRef>().is_err());
    assert!(":alpha".parse::<ItemRef>().is_err());
}
