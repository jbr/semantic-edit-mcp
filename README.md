# semantic-edit-mcp

[![codecov](https://codecov.io/gh/jbr/semantic-edit-mcp/graph/badge.svg?token=n9aj3MQGLq)](https://codecov.io/gh/jbr/semantic-edit-mcp)
[![ci][ci-badge]][ci]
[![crates.io version badge][version-badge]][crate]

[ci]: https://github.com/jbr/semantic-edit-mcp/actions?query=workflow%3ACI
[ci-badge]: https://github.com/jbr/semantic-edit-mcp/workflows/CI/badge.svg
[version-badge]: https://img.shields.io/crates/v/semantic-edit-mcp.svg?style=flat-square
[crate]: https://crates.io/crates/semantic-edit-mcp

A Model Context Protocol server for AST-aware code editing

## Supported Languages

* Rust
* Python
* JavaScript, TypeScript, JSX, TSX and JSON
* TOML
* Markdown, plain text and anything else, edited as text

None of them needs anything installed: edits are laid out and validated
in-process. Formatting is optional, and off unless you turn it on.

## Formatting

By default an edit is written as given, at the indentation of the code it
replaces or goes next to, and nothing else in the file changes. To have edited
files reformatted, pass `--format`, before or after the subcommand. It works for
`serve` and for the tool subcommands, and can be repeated:

```bash
$ semantic-edit-mcp serve --format built-in                       # every built-in formatter
$ semantic-edit-mcp serve --format rust=rustfmt                   # Rust only
$ semantic-edit-mcp serve --format rust=rustfmt --format toml=taplo
```

`LANGUAGE=FORMATTER` formats one language's files, and `built-in` formats each
language with its built-in formatter. Settings apply in turn, so a later one for
a language replaces an earlier one. A formatter reformats the whole file after
each edit, and an edit to valid code that the formatter fails on is rejected,
leaving the file untouched. The built-in formatters:

| Formatter | Formats | Needs |
|---|---|---|
| `rustfmt` | Rust, at the edition the nearest `Cargo.toml` declares (2024 when none does) | `rustfmt` on the `PATH` |
| `ruff` | Python | [`ruff`](https://docs.astral.sh/ruff/) on the `PATH` |
| `biome` | JavaScript, JSX, TypeScript, TSX and JSON | [`biome`](https://biomejs.dev/) on the `PATH` |
| `taplo` | TOML | nothing: it runs in-process |

Markdown and plain text have no formatter.

## Tools
```
  edit                   Apply an edit to a file and see the resulting diff
  retarget-edit          Move the most recent edit to a corrected anchor without resending its content
  undo-edit              Revert the most recent edit, restoring the file's prior content
  find-anchor            Show every location an anchor matches in a file, with context
```

Accepted edits persist immediately. For syntactically valid code, an edit must
leave it valid, and pass the formatter when one is configured; otherwise the
file is left untouched. For Rust, Python and TOML, valid means passing a check
stricter than the syntax tree: a full Rust parse, Python's indentation rules,
and TOML's rule against defining a key twice. A file that already fails that
check stays editable, and while it fails, each edit's response says the check
was off. A source that already has syntax errors can be repaired progressively:
anchor edits use textual ranges, still-invalid results are written without
formatting, and the response shows all remaining syntax-error regions, even
outside the diff. Once syntax is restored, the normal validity guard resumes.
Formatter failures during repair are reported without discarding the edit.

Review the returned diff: if the edit landed somewhere other than intended,
`retarget-edit` moves it and `undo-edit` reverts it. Both take the file path;
repeated undo calls walk back that file's retained edits (up to 32 edits are
held across all files). Paths are absolute or relative to the server's startup
directory, configurable with `--root`.

## Markdown and plain-text whitespace

These files are not reformatted. When replacing the first text on an indented
line, content beginning with spaces or tabs supplies the new indentation rather
than adding to the old indentation. Unindented content keeps the existing line
indentation. Continuation lines are inserted exactly as supplied; no list or
code-block nesting is inferred. To remove indentation entirely, include text
from the preceding line and the intervening newline in a larger replacement.
All whitespace-only changes remain visible in the returned diff.

## Installation

```bash
$ cargo install semantic-edit-mcp
```

## Usage with Claude Desktop or gemini-cli

Add this to your MCP configuration JSON file:

```json
{
  "mcpServers": {
    "edit": {
      "command": "/path/to/semantic-edit-mcp/semantic-edit-mcp",
      "args": ["serve"]
    }
  }
}
```

To reformat edited files with the built-in formatters, make the arguments
`["serve", "--format", "built-in"]`.


## License

<sup>
Licensed under either of <a href="LICENSE-APACHE">Apache License, Version
2.0</a> or <a href="LICENSE-MIT">MIT license</a> at your option.
</sup>

---

<sub>
Unless you explicitly state otherwise, any contribution intentionally submitted
for inclusion in this crate by you, as defined in the Apache-2.0 license, shall
be dual licensed as above, without any additional terms or conditions.
</sub>
