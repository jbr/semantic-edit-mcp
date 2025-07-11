pub(crate) fn find_positions(
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
