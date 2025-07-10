use std::{borrow::Cow, iter::Iterator};

use tree_sitter::{Node, Tree};

use crate::{
    editor::EditPosition,
    selector::{Operation, Selector},
};

use super::{Edit, Editor};

#[derive(fieldwork::Fieldwork)]
#[fieldwork(get, into)]
pub struct EditIterator<'editor, 'language> {
    editor: &'editor Editor<'language>,
    #[field(with, get_mut, set)]
    selector: Cow<'editor, Selector>,
    source_code: &'editor str,
    #[field(with, get_mut, set)]
    content: Cow<'editor, str>,
    tree: &'editor Tree,
    #[field = false]
    staged_edit: Option<&'editor EditPosition>,
    #[field(get_mut(deref = false))]
    edits: Option<Vec<Edit<'editor, 'language>>>,
    current_index: usize,
}

impl<'editor, 'language> EditIterator<'editor, 'language> {
    pub(crate) fn new(editor: &'editor Editor<'language>) -> Self {
        let Editor {
            selector,
            source_code,
            tree,
            staged_edit,
            content,
            ..
        } = &editor;
        Self {
            editor,
            selector: Cow::Borrowed(selector),
            content: Cow::Borrowed(content),
            source_code,
            tree,
            staged_edit: staged_edit.as_ref(),
            edits: None,
            current_index: 0,
        }
    }

    pub(crate) fn find_edits(&self) -> Result<Vec<Edit<'editor, 'language>>, String> {
        let source_code: &str = self.source_code;
        let tree: &Tree = self.tree;
        self.selector.validate()?;
        let Selector { operation, anchor } = &*self.selector;

        match operation {
            Operation::InsertAfter => {
                self.find_after_ast_insert_positions(anchor, source_code, tree)
            }
            Operation::InsertBefore => {
                self.find_before_ast_insert_positions(anchor, source_code, tree)
            }
            Operation::Replace => self.select_ast_node(anchor, source_code, tree),
        }
    }

    fn ensure_text_ranges_loaded(&mut self) -> Result<(), String> {
        if self.edits.is_none() {
            self.edits = Some(self.find_edits()?);
        }
        Ok(())
    }

    fn build_edit(&self, start_byte: usize) -> Edit<'editor, 'language> {
        Edit::new(
            self.editor,
            EditPosition {
                start_byte,
                end_byte: None,
            },
        )
        .with_content(self.content.clone())
    }

    fn find_after_ast_insert_positions(
        &self,
        anchor: &str,
        source_code: &str,
        tree: &'editor Tree,
    ) -> Result<Vec<Edit<'editor, 'language>>, String> {
        let mut edits = self
            .select_ast_node(anchor, source_code, tree)?
            .into_iter()
            .filter_map(Edit::insert_after)
            .collect::<Vec<_>>();

        let mut additional = vec![];
        for edit in &edits {
            additional.push(edit.clone().with_content(format!(" {}", &edit.content)));
            additional.push(edit.clone().with_content(format!("\n{}", &edit.content)));
        }
        edits.extend_from_slice(&additional);
        Ok(edits)
    }
    fn find_before_ast_insert_positions(
        &self,
        anchor: &str,
        source_code: &str,
        tree: &'editor Tree,
    ) -> Result<Vec<Edit<'editor, 'language>>, String> {
        let mut edits = self
            .select_ast_node(anchor, source_code, tree)?
            .into_iter()
            .map(Edit::insert_before)
            .collect::<Vec<_>>();

        let mut additional = vec![];
        for edit in &edits {
            additional.push(edit.clone().with_content(format!("{}\n", &edit.content)));
            additional.push(edit.clone().with_content(format!("{} ", &edit.content)));
        }
        edits.extend_from_slice(&additional);
        Ok(edits)
    }

    fn select_ast_node(
        &self,
        anchor: &str,
        source_code: &str,
        tree: &'editor Tree,
    ) -> Result<Vec<Edit<'editor, 'language>>, String> {
        let anchor = anchor.trim();
        let mut candidates = vec![];
        for (start, end) in find_positions(source_code, anchor)? {
            candidates.push(
                self.build_edit(start)
                    .with_end_byte(end)
                    .with_internal_explanation("exact"),
            );

            if let Some(parent) = tree.root_node().descendant_for_byte_range(start, end) {
                let nodes = siblings_in_range(parent, start, end);
                if !nodes.is_empty() {
                    candidates.push(
                        self.build_edit(nodes.first().as_ref().unwrap().start_byte())
                            .with_end_byte(nodes.last().as_ref().unwrap().end_byte())
                            .with_internal_explanation("node range"),
                    );
                }

                candidates.push(
                    self.build_edit(parent.start_byte())
                        .with_end_byte(parent.end_byte())
                        .with_node(parent)
                        .with_internal_explanation("common parent"),
                );
            }
        }

        Ok(candidates)
    }
}

impl<'editor, 'language> Iterator for EditIterator<'editor, 'language> {
    type Item = Result<Edit<'editor, 'language>, String>;

    fn next(&mut self) -> Option<Self::Item> {
        // If we have a staged edit, return it first and only once
        if let Some(edit_position) = self.staged_edit.take() {
            return Some(Ok(Edit::new(self.editor, *edit_position)));
        }

        // Ensure text ranges are loaded
        if let Err(e) = self.ensure_text_ranges_loaded() {
            return Some(Err(e));
        }

        // Get the current text range to try
        let text_ranges = self.edits.as_ref().unwrap();
        if self.current_index >= text_ranges.len() {
            return None; // No more ranges to try
        }

        let edit = text_ranges[self.current_index].clone();
        self.current_index += 1;

        Some(Ok(edit))
    }
}

fn siblings_in_range<'tree>(parent: Node<'tree>, start: usize, end: usize) -> Vec<Node<'tree>> {
    // Collect all named children that intersect the range
    let mut result = Vec::new();
    let mut cursor = parent.walk();

    if cursor.goto_first_child() {
        loop {
            let child = cursor.node();
            if child.is_named() && child.start_byte() < end && child.end_byte() > start {
                result.push(child);
            }
            if !cursor.goto_next_sibling() {
                break;
            }
        }
    }

    result
}

pub(super) fn find_positions(
    source_text: &str,
    snippet: &str,
) -> Result<Vec<(usize, usize)>, String> {
    let mut results = vec![];

    let (initial_anchor, remaining_snippet) = match snippet.find(char::is_whitespace) {
        Some(pos) => snippet.split_at(pos),
        None => (snippet, ""),
    };

    for (start_pos, _) in source_text.match_indices(initial_anchor) {
        let mut current_pos = start_pos + initial_anchor.len();
        if remaining_snippet.is_empty() {
            results.push((start_pos, current_pos));
            continue;
        }

        let mut snippet_chars = remaining_snippet.chars();
        let mut source_chars = source_text[current_pos..].chars();
        let mut snippet_char = snippet_chars.next();
        let mut source_char = source_chars.next();

        loop {
            match (snippet_char, source_char) {
                (Some(s), Some(src)) => {
                    if s == src {
                        snippet_char = snippet_chars.next();
                        source_char = source_chars.next();
                        current_pos += src.len_utf8();
                    } else if s.is_whitespace() {
                        snippet_char = snippet_chars.next();
                    } else if src.is_whitespace() {
                        source_char = source_chars.next();
                        current_pos += src.len_utf8();
                    } else {
                        break; // Mismatch
                    }
                }
                (None, _) => {
                    // Snippet exhausted - complete match!
                    results.push((start_pos, current_pos));
                    break;
                }
                (Some(_), None) => break, // Source exhausted, snippet remaining
            }
        }
    }
    if !results.is_empty() {
        Ok(results)
    } else {
        Err(format!("Anchor \"{snippet}\" not found in source"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn position_strs<'a>(source: &'a str, snippet: &str) -> Vec<&'a str> {
        find_positions(source, snippet)
            .unwrap_or_default()
            .into_iter()
            .map(|(start, end)| &source[start..end])
            .collect()
    }

    #[test]
    fn test_exact_match_single_line() {
        let source = "hello world\nfoo bar\nbaz";
        let snippet = "foo bar";
        let result = position_strs(source, snippet);
        assert_eq!(result, ["foo bar"]);
    }

    #[test]
    fn test_exact_match_multiline() {
        let source = "line1\nline2\nline3\nline4";
        let snippet = "line2\nline3";
        let result = position_strs(source, snippet);
        assert_eq!(result, ["line2\nline3"]);
    }

    #[test]
    fn test_whitespace_differences() {
        let source = "  hello   world  \n\t\tfoo\tbar\t\n   baz   ";
        let snippet = "hello world\nfoo bar";
        let result = position_strs(source, snippet);
        assert_eq!(result, ["hello   world  \n\t\tfoo\tbar"]);
    }

    #[test]
    fn test_multiple_matches() {
        let source = "foo\nbar\nbaz\nfoo\nbar\nqux";
        let snippet = "foo\nbar";
        let result = position_strs(source, snippet);
        assert_eq!(result, ["foo\nbar", "foo\nbar"]);
    }

    #[test]
    fn test_overlapping_first_lines() {
        let source = "abc1abc   1abc\nghi\nabc\njkl";
        let snippet = "abc 1 abc";
        let result = position_strs(source, snippet);
        assert_eq!(result, ["abc1abc", "abc   1abc"]);
    }

    #[test]
    fn test_first_line_appears_multiple_times_but_only_one_full_match() {
        let source = "start\nmiddle\nstart\nend\nother";
        let snippet = "start\nend";
        let result = position_strs(source, snippet);
        assert_eq!(result, ["start\nend"]);
    }

    #[test]
    fn test_no_matches_first_line_not_found() {
        let source = "hello\nworld\nfoo";
        let snippet = "missing\nline";
        let result = position_strs(source, snippet);
        assert!(result.is_empty());
    }

    #[test]
    fn test_no_matches_partial_match() {
        let source = "hello\nworld\nfoo";
        let snippet = "hello\nmissing";
        let result = position_strs(source, snippet);
        assert!(result.is_empty());
    }

    #[test]
    fn test_single_line_snippet() {
        let source = "one\ntwo\nthree 3\nfour";
        let snippet = "three      3";
        let result = position_strs(source, snippet);
        assert_eq!(result, ["three 3"]);
    }

    #[test]
    fn test_entire_source_matches() {
        let source = "line1\nline2\nline3";
        let snippet = "line1\nline2\nline3";
        let result = position_strs(source, snippet);
        assert_eq!(result, [source]);
    }

    #[test]
    fn test_whitespace_only_differences() {
        let source = "func(a,  b  )\n{\n    return a + b;\n}";
        let snippet = "func(a, b)\n{\nreturn a + b;\n}";
        let result = position_strs(source, snippet);
        assert_eq!(result, [source]);
    }

    #[test]
    fn test_mixed_whitespace_types() {
        let source = "hello\tworld\r\n  foo   bar  ";
        let snippet = "hello world\nfoo bar";
        let result = position_strs(source, snippet);
        assert_eq!(result, ["hello\tworld\r\n  foo   bar"]);
    }

    #[test]
    fn test_trailing_whitespace_in_source() {
        let source = "line1   \nline2\t\t\nline3";
        let snippet = "line1\nline2";
        let result = position_strs(source, snippet);
        assert_eq!(result, ["line1   \nline2"]);
    }

    #[test]
    fn test_snippet_longer_than_remaining_source() {
        let source = "short\nfile";
        let snippet = "short\nfile\nextra\nlines";
        let result = position_strs(source, snippet);
        assert!(result.is_empty());
    }

    #[test]
    fn test_unicode_characters() {
        let source = "héllo\nwörld\n测试";
        let snippet = "héllo\nwörld";
        let result = position_strs(source, snippet);
        assert_eq!(result, ["héllo\nwörld"]);
    }

    #[test]
    fn test_first_line_at_end_of_source() {
        let source = "beginning\nmiddle\nend";
        let snippet = "end\nextra";
        let result = position_strs(source, snippet);
        assert!(result.is_empty());
    }
}
