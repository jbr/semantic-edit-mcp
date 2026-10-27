//! Python's indentation rules, checked over a file's logical lines.
//!
//! tree-sitter's Python grammar accepts indentation the language rejects: a
//! block header with nothing indented under it (the empty suite parses as a
//! zero-width `block`), and a line indented or dedented where no block opens or
//! closes, which it reads as one more statement at the same level. This is the
//! language reference's "Lexical analysis → Indentation", with the one rule of
//! the grammar that rests on it, that an indented block follows a block header,
//! reported in CPython's words.
//!
//! The parse is used only to find where logical lines start and end: its tokens
//! show which line breaks fall inside brackets or strings, and which logical
//! lines end with `:`, and the text between them shows the backslashes that
//! join lines. Its block structure isn't used, because in exactly these cases
//! it is wrong.

use crate::validation::Diagnostic;
use std::ops::Range;
use tree_sitter::{Node, Tree};

/// The first indentation error in `source`, whose parse is `tree`, as CPython
/// reports it.
pub(super) fn check(tree: &Tree, source: &str) -> Result<(), Diagnostic> {
    let lines = logical_lines(tree, source);
    // The indentation of each open block, outermost first. The file's own level
    // is never popped.
    let mut levels = vec![Indent::default()];
    let mut previous: Option<&LogicalLine<'_>> = None;
    for line in &lines {
        let header = previous.filter(|previous| previous.opens_block);
        let at = line.row + 1;
        let indent = line.indent;
        let level = levels[levels.len() - 1];
        if indent.columns > level.columns {
            if indent.tab_as_one <= level.tab_as_one {
                return Err(tab_error(at));
            }
            if header.is_none() {
                return Err(indentation_error(at, "unexpected indent"));
            }
            levels.push(indent);
        } else {
            while levels.len() > 1 && indent.columns < levels[levels.len() - 1].columns {
                levels.pop();
            }
            let level = levels[levels.len() - 1];
            if indent.columns != level.columns {
                return Err(indentation_error(
                    at,
                    "unindent does not match any outer indentation level",
                ));
            }
            if indent.tab_as_one != level.tab_as_one {
                return Err(tab_error(at));
            }
            if let Some(header) = header {
                return Err(expected_block(at, header));
            }
        }
        previous = Some(line);
    }
    match previous.filter(|line| line.opens_block) {
        // At the end of the file, CPython points at its last line.
        Some(header) => Err(expected_block(source.lines().count().max(1), header)),
        None => Ok(()),
    }
}

/// A logical line: its first token's row and indentation, and what its tokens
/// say about the statement it holds.
#[derive(Debug)]
struct LogicalLine<'tree> {
    /// The row its first token is on, counting from 0.
    row: usize,
    indent: Indent,
    /// The kinds of its first two tokens, which name a block header's statement.
    first: &'tree str,
    second: Option<&'tree str>,
    /// Whether its last token is a `:`, so it is a block header whose block
    /// must follow, indented, on the lines after it.
    opens_block: bool,
}

/// The column indentation reaches, measured twice: with a tab advancing to the
/// next multiple of 8, as Python measures it, and with a tab as one column.
/// Indentation whose order differs between the two depends on how wide a tab
/// is, which is a `TabError`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Indent {
    columns: usize,
    tab_as_one: usize,
}

impl Indent {
    /// The indentation of a line beginning `whitespace`. A form feed resets it,
    /// as CPython's tokenizer does; anything but spaces, tabs and form feeds
    /// ends it.
    fn of(whitespace: &str) -> Self {
        let mut indent = Self::default();
        for c in whitespace.chars() {
            match c {
                ' ' => {
                    indent.columns += 1;
                    indent.tab_as_one += 1;
                }
                '\t' => {
                    indent.columns = indent.columns / 8 * 8 + 8;
                    indent.tab_as_one += 1;
                }
                '\x0c' => indent = Self::default(),
                _ => break,
            }
        }
        indent
    }
}

/// The logical lines of `source`, in order, from the tokens of `tree`.
///
/// A token begins a new logical line when it is outside brackets and a line
/// break that no backslash escapes separates it from the token before it. Lines
/// inside a string are inside its token, and comments and blank lines hold no
/// tokens, so none of them begins a line.
fn logical_lines<'tree>(tree: &'tree Tree, source: &str) -> Vec<LogicalLine<'tree>> {
    let mut lines: Vec<LogicalLine<'tree>> = Vec::new();
    let mut depth = 0usize;
    // Where the last token ends, and the comments since.
    let mut previous_end: Option<usize> = None;
    let mut comments = Vec::new();
    let mut last_kind = "";

    for token in tokens(tree) {
        let kind = token.kind();
        match kind {
            "comment" => {
                comments.push(token.byte_range());
                continue;
            }
            // Read from the text instead: the parse has no node for a backslash
            // its scanner skips as whitespace, as it does between the strings of
            // an implicit concatenation.
            "line_continuation" => continue,
            // Zero-width: an empty block, which is all the parse has where a
            // header's block is missing.
            _ if token.start_byte() == token.end_byte() => continue,
            _ => {}
        }

        let indentation_start = match previous_end {
            None => Some(line_break(source, 0, token.start_byte(), &comments).unwrap_or(0)),
            Some(end) if depth == 0 => line_break(source, end, token.start_byte(), &comments),
            Some(_) => None,
        };
        comments.clear();
        if let Some(indentation_start) = indentation_start {
            if let Some(line) = lines.last_mut() {
                line.opens_block = last_kind == ":";
            }
            lines.push(LogicalLine {
                row: token.start_position().row,
                indent: indentation(source, indentation_start, token.start_byte()),
                first: kind,
                second: None,
                opens_block: false,
            });
        } else if let Some(line) = lines.last_mut() {
            line.second.get_or_insert(kind);
        }

        previous_end = Some(token.end_byte());
        match kind {
            "(" | "[" | "{" => depth += 1,
            ")" | "]" | "}" => depth = depth.saturating_sub(1),
            _ => {}
        }
        last_kind = kind;
    }
    if let Some(line) = lines.last_mut() {
        line.opens_block = last_kind == ":";
    }
    lines
}

/// Whether the text between two tokens, from `from` to `to`, ends a logical
/// line: if it does, where the next line's indentation is measured from, and
/// `None` if a backslash escapes every line break in it.
///
/// Indentation is measured from the start of the row after the last unescaped
/// line break. That is `to`'s row, unless rows ending in backslashes run into
/// it: indentation can't be split over lines, so then it is the whitespace
/// before the first of those backslashes.
fn line_break(source: &str, from: usize, to: usize, comments: &[Range<usize>]) -> Option<usize> {
    source[from..to]
        .match_indices('\n')
        .map(|(offset, _)| from + offset)
        .rfind(|newline| !escaped(source, *newline, comments))
        .map(|newline| newline + 1)
}

/// Whether a backslash escapes the line break at `newline`: it ends the row,
/// outside a comment.
fn escaped(source: &str, newline: usize, comments: &[Range<usize>]) -> bool {
    let row = &source[..newline];
    let row = row.strip_suffix('\r').unwrap_or(row);
    row.ends_with('\\')
        && !comments
            .iter()
            .any(|comment| comment.contains(&(row.len() - 1)))
}

/// The tokens of `tree` in order: its leaves, with each string literal whole,
/// since the lines inside one are part of the literal.
fn tokens(tree: &Tree) -> Vec<Node<'_>> {
    let mut tokens = Vec::new();
    let mut cursor = tree.walk();
    if !cursor.goto_first_child() {
        return tokens;
    }
    loop {
        let node = cursor.node();
        if node.kind() == "string" || node.child_count() == 0 {
            tokens.push(node);
        } else if cursor.goto_first_child() {
            continue;
        }
        while !cursor.goto_next_sibling() {
            if !cursor.goto_parent() {
                return tokens;
            }
        }
    }
}

/// The indentation measured from `start`, a row's start, up to `to`. A byte
/// order mark at the start of the file isn't part of it.
fn indentation(source: &str, start: usize, to: usize) -> Indent {
    let whitespace = &source[start..to];
    let whitespace = match start {
        0 => whitespace.strip_prefix('\u{feff}').unwrap_or(whitespace),
        _ => whitespace,
    };
    Indent::of(whitespace)
}

fn indentation_error(line: usize, message: &str) -> Diagnostic {
    Diagnostic {
        message: format!("IndentationError: {message}"),
        line: Some(line),
        column: None,
    }
}

fn tab_error(line: usize) -> Diagnostic {
    Diagnostic {
        message: String::from("TabError: inconsistent use of tabs and spaces in indentation"),
        line: Some(line),
        column: None,
    }
}

/// CPython's error for `header` with no indented block after it, at `line`.
fn expected_block(line: usize, header: &LogicalLine<'_>) -> Diagnostic {
    let statement = match header.first {
        "async" => header.second,
        first => Some(first),
    };
    let after = match statement {
        Some("def") => String::from(" after function definition"),
        Some("class") => String::from(" after class definition"),
        Some("except") if header.second == Some("*") => String::from(" after 'except*' statement"),
        Some(
            keyword @ ("if" | "elif" | "else" | "for" | "while" | "try" | "except" | "except*"
            | "finally" | "with" | "match" | "case"),
        ) => format!(" after '{keyword}' statement"),
        _ => String::new(),
    };
    indentation_error(
        line,
        &format!(
            "expected an indented block{after} on line {}",
            header.row + 1
        ),
    )
}

#[cfg(test)]
mod tests;
