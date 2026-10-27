use crate::{
    editor::{Edit, EditIterator, Editor},
    languages::LanguageEditor,
    layout::Rules,
};
use anyhow::Result;

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
    /// A line inside a string (continued with a backslash), a template literal
    /// (interpolations and all) or a TypeScript template literal type is never
    /// re-indented. JSX text isn't a literal: JSX collapses the whitespace around
    /// its line breaks, so its indentation is layout like code's.
    fn layout(&self) -> Rules {
        Rules {
            literals: &["string", "template_string", "template_literal_type"],
            ..Rules::CODE
        }
    }

    fn build_edits<'language, 'editor>(
        &self,
        editor: &'editor Editor<'language>,
    ) -> Result<Vec<Edit<'editor, 'language>>, String> {
        Ok(with_comma_variants(EditIterator::new(editor).find_edits()?))
    }
}
