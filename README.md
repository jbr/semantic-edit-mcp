# semantic-edit-mcp

[![codecov](https://codecov.io/gh/jbr/semantic-edit-mcp/graph/badge.svg?token=n9aj3MQGLq)](https://codecov.io/gh/jbr/semantic-edit-mcp)
[![ci][ci-badge]][ci]
[![crates.io version badge][version-badge]][crate]

[ci]: https://github.com/jbr/semantic-edit-mcp/actions?query=workflow%3ACI
[ci-badge]: https://github.com/jbr/semantic-edit-mcp/workflows/CI/badge.svg
[version-badge]: https://img.shields.io/crates/v/semantic-edit-mcp.svg?style=flat-square
[crate]: https://crates.io/crates/semantic-edit-mcp



A Model Context Protocol server for AST-aware code editing

## Tools
```
  preview-edit           Stage an operation and see a preview of the changes
  retarget-edit          Change the targeting of an already-staged operation without rewriting the content
  persist-edit           Execute the currently staged operation
  set-working-directory  Set the working context path for a session
```

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
