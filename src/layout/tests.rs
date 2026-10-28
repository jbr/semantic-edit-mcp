use super::*;

/// Render every reading `readings` offers, in order, as `(start, text)`.
fn render_all(target: Target<'_>, content: &str) -> Vec<(usize, String)> {
    let content = Content::new(content);
    content
        .readings(&target, None)
        .into_iter()
        .map(|reading| {
            let placement = content.render(&target, reading);
            (placement.start(), placement.into_text())
        })
        .collect()
}

/// `source` with `content` inserted at `start`, after the code beginning at
/// `item`, laid out at file columns.
fn insert(source: &str, start: usize, item: Option<usize>, content: &str) -> String {
    insert_with(source, start, item, None, &Rules::CODE, content)
}

fn insert_with(
    source: &str,
    start: usize,
    item: Option<usize>,
    tree: Option<&Tree>,
    rules: &Rules,
    content: &str,
) -> String {
    let (target, content) = Target::inserting(source, start, item, tree, rules, content);
    Content::new(content)
        .render(&target, Reading::FileColumns)
        .splice(source, None)
}

fn parse(language: tree_sitter::Language, source: &str) -> Tree {
    let mut parser = Parser::new();
    parser.set_language(&language).unwrap();
    parser.parse(source, None).unwrap()
}

const BODY: &str = "def f():\n    old()\n    keep()\n";

#[test]
fn a_bare_first_line_goes_in_after_the_line_indentation() {
    let start = BODY.find("old").unwrap();
    let target = Target::replacing(BODY, start);
    let readings = render_all(target, "new()\n    more()");
    assert_eq!(
        readings,
        [
            (start, "new()\n    more()".into()),
            (start, "new()\n        more()".into()),
        ]
    );
}

#[test]
fn leading_whitespace_on_the_first_line_replaces_the_line_indentation() {
    let start = BODY.find("old").unwrap();
    let target = Target::replacing(BODY, start);
    let readings = render_all(target, "  new()\n    more()");
    assert_eq!(readings, [(start - 4, "  new()\n    more()".into())]);
}

#[test]
fn continuation_lines_left_of_the_target_read_as_dedented_first() {
    let start = BODY.find("old").unwrap();
    let target = Target::replacing(BODY, start);
    let readings = render_all(target, "new()\nmore()");
    assert_eq!(
        readings,
        [
            (start, "new()\n    more()".into()),
            (start, "new()\nmore()".into()),
        ]
    );
}

#[test]
fn the_anchors_convention_outranks_the_contents_evidence() {
    let start = BODY.find("old").unwrap();
    let target = Target::replacing(BODY, start);
    let content = Content::new("new()\nmore()");
    assert_eq!(
        content.readings(&target, Some(Reading::FileColumns)),
        [Reading::FileColumns, Reading::Dedented]
    );
}

/// An anchor that runs out of its target's block at the file's columns shows
/// the content's lines left of the target are at file columns too; one that
/// stays in the block, or was flattened, shows nothing.
#[test]
fn an_anchor_leaving_its_block_at_file_columns_shows_file_columns() {
    let source = "class A:\n    def m(self):\n        return 1\n\n\ndef helper():\n    pass\n";
    let start = source.find("return 1").unwrap();
    let end = source.len() - 1;
    let hit = Some((start, end));
    let leaving = "return 1\n\n\ndef helper():\n    pass";
    assert_eq!(
        anchor_convention(Some(leaving), source, hit),
        Some(Reading::FileColumns)
    );
    let flattened = "return 1\n\n\ndef helper():\npass";
    assert_eq!(anchor_convention(Some(flattened), source, hit), None);

    let start = source.find("def m").unwrap();
    let end = source.find("return 1").unwrap() + "return 1".len();
    let staying = "def m(self):\n        return 1";
    assert_eq!(
        anchor_convention(Some(staying), source, Some((start, end))),
        None
    );
    assert_eq!(
        anchor_convention(Some("def m(self):"), source, Some((start, start + 12))),
        None
    );
}

#[test]
fn verbatim_lines_neither_move_nor_count() {
    let start = BODY.find("old").unwrap();
    let target = Target::replacing(BODY, start);
    let mut content = Content::new("s = \"\"\"\\\nraw\n\"\"\"\nmore()");
    content.set_verbatim(vec![false, true, true, false]);
    let dedented = content.render(&target, Reading::Dedented);
    assert_eq!(dedented.text(), "s = \"\"\"\\\nraw\n\"\"\"\n    more()");

    // With only verbatim lines after it, there is nothing to read either way.
    let mut content = Content::new("s = \"\"\"\\\nraw\n\"\"\"");
    content.set_verbatim(vec![false, true, true]);
    assert_eq!(content.readings(&target, None), [Reading::FileColumns]);
}

#[test]
fn an_insert_above_a_line_leaves_that_line_where_it_was() {
    let start = BODY.find("keep").unwrap();
    // However the content ends, the anchor's line keeps its indentation, and
    // content ending in code goes on a line of its own.
    for content in ["new()\n", "new()\n    ", "new()\n        ", "new()"] {
        assert_eq!(
            insert(BODY, start, None, content),
            "def f():\n    old()\n    new()\n    keep()\n",
            "{content:?}"
        );
    }
    // An inline prefix stays inline.
    assert_eq!(
        insert(BODY, start, None, "async "),
        "def f():\n    old()\n    async keep()\n"
    );
    // Content with its own indentation takes the whole line.
    assert_eq!(
        insert(BODY, start, None, "    new()"),
        "def f():\n    old()\n    new()\n    keep()\n"
    );
}

#[test]
fn an_insert_anywhere_in_a_lines_indentation_goes_above_its_first_token() {
    // `insert_after` a node that ends with its line break, like a TOML table or
    // a Rust doc comment, lands at the next line's start.
    let start = BODY.find("    keep").unwrap();
    assert_eq!(
        insert(BODY, start, None, "new()"),
        "def f():\n    old()\n    new()\n    keep()\n"
    );
    // At the end of a file that ends with a line break, the content becomes the
    // file's last lines, and ends with one too.
    assert_eq!(
        insert(BODY, BODY.len(), None, "new()"),
        format!("{BODY}new()\n")
    );
}

#[test]
fn content_after_a_line_starts_on_its_own_line() {
    let item = BODY.find("old()").unwrap();
    let start = item + "old()".len();
    assert_eq!(
        insert(BODY, start, Some(item), "new()\n    more()"),
        "def f():\n    old()\n    new()\n    more()\n    keep()\n"
    );
}

#[test]
fn a_leading_line_break_is_the_one_the_seam_would_add() {
    let item = BODY.find("keep()").unwrap();
    let start = item + "keep()".len();
    for (content, inserted) in [
        ("new()", "\n    new()"),
        ("\nnew()", "\n    new()"),
        ("\n\nnew()", "\n\n    new()"),
        ("\n    new()", "\n    new()"),
        ("\n", "\n"),
    ] {
        let expected = format!("def f():\n    old()\n    keep(){inserted}\n");
        assert_eq!(
            insert(BODY, start, Some(item), content),
            expected,
            "{content:?}"
        );
    }
}

#[test]
fn leading_blank_lines_come_before_the_first_line() {
    let item = BODY.find("keep()").unwrap();
    let start = item + "keep()".len();
    let (target, content) = Target::inserting(
        BODY,
        start,
        Some(item),
        None,
        &Rules::CODE,
        "\n\ndef g():\n        pass",
    );
    let readings = render_all(target, content);
    assert_eq!(
        readings[0],
        (start, "\n\n    def g():\n        pass".into())
    );
}

#[test]
fn an_insert_inside_a_line_stays_inline() {
    let source = "fn f(a: u8, b: u8) {}\n";
    let item = source.find("a: u8").unwrap();
    let start = item + "a: u8".len();
    assert_eq!(
        insert(source, start, Some(item), ", x: u8"),
        "fn f(a: u8, x: u8, b: u8) {}\n"
    );
    // So does one after code that begins partway through its line.
    let source = "let x = 1; let y = 2;\n";
    let item = source.find("let y").unwrap();
    assert_eq!(
        insert(source, source.len() - 1, Some(item), "let z = 3;"),
        "let x = 1; let y = 2;let z = 3;\n"
    );
}

#[test]
fn a_separator_ending_the_line_stays_with_the_item_it_follows() {
    let source = "{\n    a: 1,\n    b: 2\n}\n";
    let item = source.find("a: 1").unwrap();
    let start = item + "a: 1".len();
    // Content goes below the line, after its separator …
    assert_eq!(
        insert(source, start, Some(item), "x: 3,"),
        "{\n    a: 1,\n    x: 3,\n    b: 2\n}\n"
    );
    // … unless it brings its own: that one ends the line, and the line's own
    // separator follows the content.
    assert_eq!(
        insert(source, start, Some(item), ", x: 3"),
        "{\n    a: 1,\n    x: 3,\n    b: 2\n}\n"
    );
    // With no separator on the line, the content's is put on it.
    let item = source.find("b: 2").unwrap();
    let start = item + "b: 2".len();
    assert_eq!(
        insert(source, start, Some(item), ",x: 3"),
        "{\n    a: 1,\n    b: 2,\n    x: 3\n}\n"
    );
    // Content that is just a separator stays on the line.
    assert_eq!(
        insert(source, start, Some(item), ","),
        "{\n    a: 1,\n    b: 2,\n}\n"
    );
}

/// After a statement or a whole definition, a `,` can still parse (Python reads
/// `return a + b` followed by `, c` as a tuple), but it isn't the separator of
/// a list the code is in, so it doesn't end that line.
#[test]
fn only_a_list_element_takes_a_leading_separator() {
    let source = "def add(a, b):\n    return a + b\n\n\nx = 1\n";
    let item = 0;
    let start = source.find("a + b").unwrap() + "a + b".len();
    assert_eq!(
        insert(source, start, Some(item), ", c"),
        "def add(a, b):\n    return a + b\n, c\n\n\nx = 1\n"
    );
}

#[test]
fn a_trailing_comment_stays_with_the_line_it_ends() {
    let source = "fn f() {\n    a(); // why\n    b();\n}\n";
    let tree = parse(tree_sitter_rust::LANGUAGE.into(), source);
    let item = source.find("a();").unwrap();
    let start = item + "a();".len();
    assert_eq!(
        insert_with(source, start, Some(item), Some(&tree), &Rules::CODE, "x();"),
        "fn f() {\n    a(); // why\n    x();\n    b();\n}\n"
    );
    // Without a tree to show it is a comment, the rest of the line is code.
    assert_eq!(
        insert(source, start, Some(item), "x();"),
        "fn f() {\n    a();x(); // why\n    b();\n}\n"
    );
}

#[test]
fn prose_continues_a_line_the_way_words_follow_one_another() {
    let source = "- one\n- two, three\n";
    let item = source.find("- two").unwrap();
    let start = source.find('\n').unwrap() + "\n- two, three".len();
    let rules = Rules::PROSE;
    assert_eq!(
        insert_with(source, start, Some(item), None, &rules, " four"),
        "- one\n- two, three four\n"
    );
    assert_eq!(
        insert_with(source, start, Some(item), None, &rules, ", four"),
        "- one\n- two, three, four\n"
    );
    assert_eq!(
        insert_with(source, start, Some(item), None, &rules, "- four"),
        "- one\n- two, three\n- four\n"
    );
    // Indentation begins a line of its own.
    assert_eq!(
        insert_with(source, start, Some(item), None, &rules, "  - four"),
        "- one\n- two, three\n  - four\n"
    );
    // Punctuation in prose isn't a separator: the line continues past it.
    let start = source.find(", three").unwrap();
    assert_eq!(
        insert_with(source, start, Some(item), None, &rules, " and"),
        "- one\n- two and, three\n"
    );
}

#[test]
fn inserted_text_takes_the_files_line_endings() {
    let source = "fn f() {\r\n    a();\r\n    b();\r\n}\r\n";
    let item = source.find("a();").unwrap();
    let start = item + "a();".len();
    assert_eq!(
        insert(source, start, Some(item), "x();\n    y();"),
        "fn f() {\r\n    a();\r\n    x();\r\n    y();\r\n    b();\r\n}\r\n"
    );
    let start = source.find("b();").unwrap();
    assert_eq!(
        insert(source, start, None, "x();\r\n"),
        "fn f() {\r\n    a();\r\n    x();\r\n    b();\r\n}\r\n"
    );
    let target = Target::replacing(source, item);
    let placement = Content::new("x();\n    y();").render(&target, Reading::FileColumns);
    assert_eq!(
        placement.splice(source, Some(item + "a();".len())),
        "fn f() {\r\n    x();\r\n    y();\r\n    b();\r\n}\r\n"
    );
}

#[test]
fn a_replacement_through_a_line_break_doesnt_add_another() {
    let source = "a();\nb();\nc();\n";
    let end = source.find("b();").unwrap() + "b();".len();
    assert_eq!(
        through_line_end("x();\n", source, end, Some("b();\n")),
        "x();"
    );
    assert_eq!(
        through_line_end("x();\n  ", source, end, Some("b();\n")),
        "x();"
    );
    // The content's own blank line stays.
    assert_eq!(
        through_line_end("x();\n\n", source, end, Some("b();\n")),
        "x();\n"
    );
    // Without the anchor's line break, the content's break is its own.
    assert_eq!(
        through_line_end("x();\n", source, end, Some("b();")),
        "x();\n"
    );
    // And a replacement ending partway through a line keeps it.
    let end = source.find("b(").unwrap() + 1;
    assert_eq!(through_line_end("x\n", source, end, Some("b\n")), "x\n");
}

/// `source` without the deletion of `anchor` (its first match).
fn delete(source: &str, anchor: &str) -> String {
    let start = source.find(anchor).unwrap();
    let (start, end) = deletion(source, start, start + anchor.len());
    [&source[..start], &source[end..]].concat()
}

#[test]
fn deleting_whole_lines_leaves_no_empty_line() {
    let source = "fn f() {\n    let a = 1;\n    eprintln!(\"{a}\");\n    let b = 2;\n}\n";
    assert_eq!(
        delete(source, "eprintln!(\"{a}\");"),
        "fn f() {\n    let a = 1;\n    let b = 2;\n}\n"
    );
    let crlf = source.replace('\n', "\r\n");
    assert_eq!(
        delete(&crlf, "eprintln!(\"{a}\");"),
        "fn f() {\r\n    let a = 1;\r\n    let b = 2;\r\n}\r\n"
    );
    // A node that ends with its line break, like a doc comment, takes its
    // line's indentation with it.
    let source = "mod m {\n    /// Docs.\n    fn f() {}\n}\n";
    assert_eq!(delete(source, "/// Docs.\n"), "mod m {\n    fn f() {}\n}\n");
}

#[test]
fn deleting_inside_a_line_removes_just_the_text() {
    let source = "    pub fn f() {}\n";
    assert_eq!(delete(source, "pub "), "    fn f() {}\n");
    assert_eq!(delete(source, "{}"), "    pub fn f() \n");
}

#[test]
fn a_deleted_items_blank_lines_dont_double_up() {
    let source = "fn a() {}\n\nfn b() {}\n\nfn c() {}\n";
    assert_eq!(delete(source, "fn b() {}"), "fn a() {}\n\nfn c() {}\n");
    // The longer run is the separation the code around uses.
    let source =
        "class A:\n    def a(self):\n        pass\n\n    def b(self):\n        pass\n\n\nx = 1\n";
    assert_eq!(
        delete(source, "def b(self):\n        pass"),
        "class A:\n    def a(self):\n        pass\n\n\nx = 1\n"
    );
    // Blank lines don't open the file or close a block or the file.
    let source = "fn a() {}\n\nfn b() {}\n";
    assert_eq!(delete(source, "fn a() {}"), "fn b() {}\n");
    assert_eq!(delete(source, "fn b() {}"), "fn a() {}\n");
    let source = "impl A {\n    fn a() {}\n\n    fn b() {}\n}\n";
    assert_eq!(delete(source, "fn b() {}"), "impl A {\n    fn a() {}\n}\n");
    // Without blank lines on both sides, the ones on either side stay.
    let source = "a\n\nb\nc\n";
    assert_eq!(delete(source, "b"), "a\n\nc\n");
}

#[test]
fn module_level_content_is_always_as_written() {
    let source = "x = 1\n";
    let target = Target::replacing(source, 0);
    assert_eq!(
        render_all(target, "if y:\n    z = 1"),
        [(0, "if y:\n    z = 1".into())]
    );
}

/// A wrong `InputEdit` doesn't fail: tree-sitter silently reuses subtrees at
/// the wrong bytes, so the incremental parse must match a fresh one exactly.
#[test]
fn a_reparse_matches_a_fresh_parse() {
    fn nodes(tree: &Tree) -> Vec<(String, usize, usize, Point, Point)> {
        let mut nodes = vec![];
        let mut cursor = tree.walk();
        'walk: loop {
            let node = cursor.node();
            nodes.push((
                node.kind().to_string(),
                node.start_byte(),
                node.end_byte(),
                node.start_position(),
                node.end_position(),
            ));
            if cursor.goto_first_child() || cursor.goto_next_sibling() {
                continue;
            }
            while cursor.goto_parent() {
                if cursor.goto_next_sibling() {
                    continue 'walk;
                }
            }
            break nodes;
        }
    }

    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .unwrap();
    // Code after the splice is what an incremental parse reuses, and so where
    // a wrong edit range would show.
    let source = format!("{BODY}{}", "\n\ndef later():\n    return 1\n".repeat(20));
    let original = parser.parse(&source, None).unwrap();
    let old = BODY.find("old").unwrap();
    let keep = BODY.find("keep").unwrap();
    let inserting =
        |start, item, content| Target::inserting(&source, start, item, None, &Rules::CODE, content);
    let cases = [
        (
            (Target::replacing(&source, old), "s = \"\"\"\\\nx\n\"\"\""),
            Some(old + "old()".len()),
        ),
        (
            (Target::replacing(&source, old), "  new()"),
            Some(old + "old()".len()),
        ),
        (inserting(keep, None, "if x:\n    y()\n"), None),
        (inserting(old + 5, Some(old), "\n\nz = 1"), None),
        (inserting(old + 5, Some(old), "z = 1"), None),
        (inserting(old + 4, None, "\n    1,\n"), None),
    ];
    for ((target, content), end) in cases {
        let placement = Content::new(content).render(&target, Reading::FileColumns);
        let reparsed = placement
            .reparse(&mut parser, &original, &source, end)
            .unwrap();
        let fresh = parser.parse(placement.splice(&source, end), None).unwrap();
        assert_eq!(nodes(&reparsed), nodes(&fresh), "{content:?}");
    }
}

#[test]
fn tabs_reach_the_next_multiple_of_eight() {
    assert_eq!(width("\t"), 8);
    assert_eq!(width("  \t"), 8);
    assert_eq!(width("\t  "), 10);
    assert_eq!(width("    "), 4);
}
