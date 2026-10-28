//! Each expected error is CPython 3.12's, message and line, for the same source
//! (compiled from bytes, as a file is).

use super::check;
use tree_sitter::{Parser, Tree};

fn parse(source: &str) -> Tree {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .unwrap();
    parser.parse(source, None).unwrap()
}

/// The check's verdict on `source`, as the editor shows its diagnostic.
fn verdict(source: &str) -> Result<(), String> {
    check(&parse(source), source).map_err(|diagnostic| diagnostic.to_string())
}

#[track_caller]
fn accepted(source: &str) {
    assert_eq!(verdict(source), Ok(()), "{source}");
}

#[track_caller]
fn rejected(source: &str, error: &str) {
    assert_eq!(verdict(source), Err(error.to_string()), "{source}");
}

/// What the check is for: tree-sitter parses each of these without an error
/// node, and Python rejects each.
#[test]
fn tree_sitter_accepts_what_python_rejects() {
    for (source, error) in [
        (
            "def load(path):\n    with open(path) as handle:\n    data = handle.read()\n",
            "line 3: IndentationError: expected an indented block after 'with' statement on line 2",
        ),
        (
            "x = 1\n    y = 2\n",
            "line 2: IndentationError: unexpected indent",
        ),
        (
            "if x:\n        a = 1\n    b = 2\n",
            "line 3: IndentationError: unindent does not match any outer indentation level",
        ),
        (
            "if x:\n\tif y:\n        pass\n",
            "line 3: TabError: inconsistent use of tabs and spaces in indentation",
        ),
    ] {
        assert!(!parse(source).root_node().has_error(), "{source}");
        rejected(source, error);
    }
}

#[test]
fn nested_blocks_and_dedents() {
    accepted("");
    accepted("x = 1\n");
    accepted(
        "def f():\n    if x:\n        if y:\n            pass\n    return 1\nz = 1\n\n\nclass A:\n    '''doc'''\n",
    );
}

#[test]
fn lines_inside_brackets_are_not_indentation() {
    accepted(concat!(
        "x = foo(1,\n",
        "  2,\n",
        "        3)\n",
        "y = [\n",
        "1,\n",
        "]\n",
        "z = {\n",
        "    'a': 1,\n",
        "      'b': 2}\n",
        "def f(\n",
        "    a,\n",
        "):\n",
        "    pass\n",
        "if (a and\n",
        "b):\n",
        "    pass\n",
        "x = {1:\n",
        "2}\n",
    ));
    accepted("with (\n    open(a) as b,\n    open(c) as d,\n):\n    pass\n");
    accepted("def f(a=lambda: 1,\n      b=2):\n    pass\n");
}

#[test]
fn lines_inside_strings_are_not_indentation() {
    accepted(concat!(
        "def f():\n",
        "    x = \"\"\"\n",
        "no indentation here\n",
        "        nor here\n",
        "\"\"\"\n",
        "    y = f\"\"\"{\n",
        "  x\n",
        "}\"\"\"\n",
        "    z = ('a'\n",
        "  'b')\n",
        "    w = 'abc\\\n",
        "def'\n",
        "    return x\n",
    ));
    // A string's last line holds the next token too, on the same logical line.
    accepted("x = '''a\nb'''; y = 1\nif x:\n    pass\n");
    // PEP 701: an f-string's quotes inside its replacement fields.
    accepted("def f():\n    return f\"{x[\"a\"]}\"\n");
}

#[test]
fn a_backslash_continues_a_line() {
    accepted("x = 1 + \\\n        2\nif a and \\\n   b:\n    pass\n");
    // The parse has no node for this backslash: its scanner skips it while
    // looking for the next string.
    accepted(concat!(
        "def f():\n",
        "    return \"a\" \\\n",
        "           \"b\"\n",
        "    x = 1\n",
    ));
    // A backslash in a comment escapes nothing.
    rejected(
        "if x:  # \\\npass\n",
        "line 2: IndentationError: expected an indented block after 'if' statement on line 1",
    );
    // Before a line's first token, it fixes the line's indentation at the
    // whitespace before it, which can't be split over lines.
    accepted("if x:\n    \\\n  y = 1\n");
    accepted("if x:\n    y = 1\n    \\\n  z = 2\n");
    rejected(
        "if x:\n    y = 1\n  \\\n    z = 2\n",
        "line 4: IndentationError: unindent does not match any outer indentation level",
    );
}

#[test]
fn comments_and_blank_lines_are_not_indentation() {
    accepted(concat!(
        "def f():\n",
        "    x = 1\n",
        "  # a comment at no level\n",
        "        # nor this\n",
        "\t\n",
        "    y = 2  # trailing\n",
        "    if x:  # header comment\n",
        "        # comment before the body\n",
        "        pass\n",
    ));
    // A comment isn't a block.
    rejected(
        "def f():\n    # todo\n",
        "line 2: IndentationError: expected an indented block after function definition on line 1",
    );
}

#[test]
fn tabs() {
    accepted("if x:\n\tif y:\n\t\tpass\n");
    // Mixed, but in the same order however wide a tab is.
    accepted("if x:\n    \tif y:\n    \t    pass\n");
    rejected(
        "if x:\n        if y:\n\t\t\tpass\n\tz\n",
        "line 3: TabError: inconsistent use of tabs and spaces in indentation",
    );
    rejected(
        "if x:\n\tif y:\n\t\tpass\n        z\n",
        "line 4: TabError: inconsistent use of tabs and spaces in indentation",
    );
}

#[test]
fn a_form_feed_resets_indentation() {
    accepted("if x:\n\x0c    y = 1\n");
    rejected(
        "if x:\n    \x0cy = 1\n",
        "line 2: IndentationError: expected an indented block after 'if' statement on line 1",
    );
}

#[test]
fn match_and_case_are_soft_keywords() {
    accepted(
        "match = 1\nmatch(x)\nmatch x:\n    case [1, 2]:\n        pass\n    case {\"a\": 1} if match:\n        pass\n    case _:\n        pass\n",
    );
    rejected(
        "match x:\ny\n",
        "line 2: IndentationError: expected an indented block after 'match' statement on line 1",
    );
    rejected(
        "match x:\n    case 1:\n    y\n",
        "line 3: IndentationError: expected an indented block after 'case' statement on line 2",
    );
}

#[test]
fn single_line_suites() {
    accepted(concat!(
        "if x: pass\n",
        "else: pass\n",
        "class A: x = 1; y = 2\n",
        "while 0: pass  # c\n",
        "def f(): ...\n",
        "try: pass\n",
        "except E: pass\n",
        "finally: pass\n",
        "if (n := 10) > 5: n\n",
    ));
    rejected(
        "if x: pass\n    y = 1\n",
        "line 2: IndentationError: unexpected indent",
    );
}

#[test]
fn decorators_and_async() {
    accepted(
        "@decorator\n@other(1,\n       2)\nasync def f():\n    async with a:\n        pass\n    async for b in c:\n        pass\n",
    );
    rejected(
        "@d\n    def f(): pass\n",
        "line 2: IndentationError: unexpected indent",
    );
    rejected(
        "async def f():\nx\n",
        "line 2: IndentationError: expected an indented block after function definition on line 1",
    );
    rejected(
        "async def f():\n    async with a:\n    x\n",
        "line 3: IndentationError: expected an indented block after 'with' statement on line 2",
    );
    rejected(
        "async def f():\n    async for a in b:\n    x\n",
        "line 3: IndentationError: expected an indented block after 'for' statement on line 2",
    );
}

#[test]
fn an_empty_block_is_named_by_its_header() {
    for (source, error) in [
        (
            "class A:\n    def f(self):\ny = 1\n",
            "line 3: IndentationError: expected an indented block after function definition on line 2",
        ),
        (
            "class A:\nx = 1\n",
            "line 2: IndentationError: expected an indented block after class definition on line 1",
        ),
        (
            "if x:\n    pass\nelse:\nx = 1\n",
            "line 4: IndentationError: expected an indented block after 'else' statement on line 3",
        ),
        (
            "if a:\n    pass\nelif b:\nx\n",
            "line 4: IndentationError: expected an indented block after 'elif' statement on line 3",
        ),
        (
            "try:\nx = 1\n",
            "line 2: IndentationError: expected an indented block after 'try' statement on line 1",
        ),
        (
            "try:\n    pass\nexcept E:\nx\n",
            "line 4: IndentationError: expected an indented block after 'except' statement on line 3",
        ),
        (
            "try:\n    pass\nexcept* E:\nx\n",
            "line 4: IndentationError: expected an indented block after 'except*' statement on line 3",
        ),
        (
            "try:\n    pass\nfinally:\nx\n",
            "line 4: IndentationError: expected an indented block after 'finally' statement on line 3",
        ),
        (
            "for a in b:\nx\n",
            "line 2: IndentationError: expected an indented block after 'for' statement on line 1",
        ),
        (
            "while a:\nx\n",
            "line 2: IndentationError: expected an indented block after 'while' statement on line 1",
        ),
        (
            "def f():\n    if x:\nprint()\n",
            "line 3: IndentationError: expected an indented block after 'if' statement on line 2",
        ),
    ] {
        rejected(source, error);
    }
}

/// At the end of the file, CPython points at its last line.
#[test]
fn an_empty_block_at_the_end_of_the_file() {
    rejected(
        "if x:\n",
        "line 1: IndentationError: expected an indented block after 'if' statement on line 1",
    );
    rejected(
        "if x:\n\n\n",
        "line 3: IndentationError: expected an indented block after 'if' statement on line 1",
    );
    rejected(
        "def f():\n    if x:\n        pass\n    else:\n",
        "line 4: IndentationError: expected an indented block after 'else' statement on line 4",
    );
}

#[test]
fn unexpected_indent_and_unmatched_dedent() {
    rejected("    x = 1\n", "line 1: IndentationError: unexpected indent");
    rejected(
        "def f():\n    x = 1\n        y = 2\n",
        "line 3: IndentationError: unexpected indent",
    );
    rejected(
        "def f():\n    if x:\n        pass\n  y = 1\n",
        "line 4: IndentationError: unindent does not match any outer indentation level",
    );
}

#[test]
fn the_first_error_is_the_one_reported() {
    rejected(
        "x = 1\n    y = 2\nif z:\n",
        "line 2: IndentationError: unexpected indent",
    );
}

#[test]
fn crlf_and_a_byte_order_mark() {
    accepted("if x:\r\n    y = 1\r\n");
    rejected(
        "if x:\r\ny = 1\r\n",
        "line 2: IndentationError: expected an indented block after 'if' statement on line 1",
    );
    accepted("\u{feff}x = 1\n");
    rejected(
        "\u{feff}    x = 1\n",
        "line 1: IndentationError: unexpected indent",
    );
}
