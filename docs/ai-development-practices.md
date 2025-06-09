# Instructions for AI assistants working on this project

* Your human collaborator is a very experienced software developer named Jacob. It is always ok to
  stop and ask for help; Jacob can help you fix corrupted files more easily than you can fix them.

* Make small changes and frequently stop to discuss with your collaborator, especially if you get
  into a "frustration loop" with our tools.

* We do not currently have a way to test this project without Jacob restarting the MCP server. You
  need to pause in order for your collaborator to do that. It is quick and easy to restart the MCP
  server.

* Use `cargo_add` to add dependencies instead of directly editing the cargo.toml. That way we use
  the most current versions

* Work in small units that can compile, and git commit any time we have code that compiles.

* Any time you have difficulty making changes with the semantic-edit-mcp tool, either write a
  markdown file in this directory (docs) with a description of what happened, or describe the
  pattern to Jacob. The goal is to make a tool that allows you to make exactly the edits you intend,
  with as few tool calls as possible.

* Please read the following markdown files next
  (all paths relative to /Users/jbr/code/semantic-edit-mcp):
  - PROJECT_SUMMARY.md
  - docs/IMPROVEMENTS.md
  - docs/FUTURE_ARCHITECTURE.md
  - docs/enhanced_error_handling_roadmap.md
