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
  edit                   Apply a validated edit to a file and see the resulting diff
  retarget-edit          Move the most recent edit to a corrected anchor without resending its content
  undo-edit              Revert the most recent edit, restoring the file's prior content
  find-anchor            Show every location an anchor matches in a file, with context
  set-working-directory  Set the working context path for a session
```

Edits persist immediately when they validate (parse + formatter). The returned
diff is for review-after-the-fact: if the edit landed somewhere other than
intended, `retarget-edit` moves it and `undo-edit` (single-level) reverts it.

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
