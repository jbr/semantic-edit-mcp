# semantic-edit-mcp

[![codecov](https://codecov.io/gh/jbr/semantic-edit-mcp/graph/badge.svg?token=n9aj3MQGLq)](https://codecov.io/gh/jbr/semantic-edit-mcp)
[![ci][ci-badge]][ci]
[![crates.io version badge][version-badge]][crate]

[ci]: https://github.com/jbr/semantic-edit-mcp/actions?query=workflow%3ACI
[ci-badge]: https://github.com/jbr/semantic-edit-mcp/workflows/CI/badge.svg
[version-badge]: https://img.shields.io/crates/v/semantic-edit-mcp.svg?style=flat-square
[crate]: https://crates.io/crates/semantic-edit-mcp

A Model Context Protocol server for AST-aware code editing

## Supported Languages and Important Language Notes

* Rust
  - Must have `rustfmt` available on the `PATH`
  - Currently assumes edition 2024 for formatting, eventually this will be determined from context
* Python
  - Must have [`ruff`](https://docs.astral.sh/ruff/) available on the `PATH`
* JavaScript/TypeScript/JSON/JSX/TSX
  - Must have [`biome`](https://biomejs.dev/) available on the `PATH`
* TOML
  - No external dependencies, taplo formatting included
* Plaintext / markdown / default editor
  - No external dependencies

## Tools
```
  edit                   Apply an edit to a file and see the resulting diff
  retarget-edit          Move the most recent edit to a corrected anchor without resending its content
  undo-edit              Revert the most recent edit, restoring the file's prior content
  find-anchor            Show every location an anchor matches in a file, with context
```

Accepted edits persist immediately. For syntactically valid code, an edit must
still parse and pass the formatter; otherwise the file is left untouched. A
source that already has syntax errors can be repaired progressively: anchor
edits use textual ranges, still-invalid results are written without formatting,
and the response shows all remaining syntax-error regions, even outside the
diff. Once syntax is restored, the normal validity guard resumes. Formatter
failures during repair are reported without discarding the edit.

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
