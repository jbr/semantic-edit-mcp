# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.4.0] - 2026-10-27

Formatting is now optional and off by default. Edits are laid out, validated and written
entirely in-process, so the server needs no external tools, and an edit changes nothing
outside itself. A configured formatter, by default, changes only the edited code. For the
behavior closest to 0.3.0's, start the server with `--format built-in --format-whole-file`,
or embed with `Formatting::built_in().with_whole_file(true)`.

### Added

- Formatters are configured per language. The standalone server takes
  `--format LANGUAGE=FORMATTER` (repeatable) or `--format built-in`, before or after the
  subcommand. The built-in set is `rustfmt` for Rust, `ruff` for Python, `biome` for
  JavaScript, TypeScript, JSX, TSX and JSON, and `taplo` (in-process) for TOML.
  Embedders pass a `Formatting` to `SemanticEditTools::with_formatting`, or replace it on
  a running instance with `set_formatting`. `Formatter::command(program, args)` runs any
  program that reads the source on stdin and writes the result to stdout, with `{path}`
  replaced by the file's path.
- Formatting settings deserialize from a strict TOML or JSON table. Unknown keys,
  misspelled languages and unknown formatters are errors.

  ```toml
  built-in = true          # every language gets its built-in formatter...
  json = false             # ...except JSON
  python = { command = "uvx", args = ["ruff", "format", "--stdin-filename", "{path}", "-"] }
  whole-file = false
  ```

  `FormattingLayer` reads the same table as a partial layer, for settings that override
  earlier ones. Both types provide a JSON schema, from `Formatting::schema()` or derived
  inside an embedder's own `JsonSchema` config type. The schema's descriptions read
  correctly in an embedder's schema.
- A formatter's changes are limited to the edit by default. The formatter sees the whole
  file, but only its changes to the edited lines and the line on either side are kept.
  When it moves the edit's own text, as rustfmt does sorting an inserted `use` or `mod`,
  the text lands where it sorts and the response says so. `--format-whole-file`
  (`Formatting::with_whole_file(true)`) reformats the whole file instead.
- In-process validation that doesn't depend on a formatter:
  - a Rust edit must leave the file parseable by `syn`;
  - a Python edit must satisfy the language's indentation rules (no empty blocks,
    unexpected indents, unmatched dedents, or ambiguous tabs and spaces), reported with
    CPython's messages;
  - a TOML edit must not define a key twice.

  Like the context checks, these block an edit only if the file passed them before it.
  When a file already fails one, the response says the check was off for that edit.
- Files that already have syntax errors can be repaired step by step.
  - Edits are accepted while the file is broken, and a result that still doesn't parse is
    written unformatted.
  - The response lists every remaining error region, including any outside the diff.
  - Once the file parses again, edits must keep it valid as usual.

  Previously such files refused every edit.
- `--root` sets the directory relative paths resolve against. It defaults to the server's
  working directory.

### Changed

- **Breaking.** Edits are no longer formatted unless formatting is configured. Unformatted
  content is written as given, at the indentation of the code it replaces or goes next
  to.
- **Breaking.** A missing or broken formatter no longer makes a language uneditable:
  - a formatter that can't run (missing from `PATH`, or timed out) is skipped, with a
    note in the response;
  - so is one that already fails on the file before the edit;
  - only a failure the edit itself causes rejects the edit.
- The formatter no longer influences where an edit lands. Validation alone picks the
  placement, and the formatter runs once, on the result.
- Formatters run in the edited file's directory and are given its path. A project's
  `rustfmt.toml`, ruff configuration and `biome.json` now apply, and ruff respects the
  project's exclusions. biome's indentation flags are passed only when no biome
  configuration governs the file. A formatter run times out after 10 seconds. On Windows,
  formatters installed by npm (`.cmd` shims) are found.
- Content is laid out by one rule in every language, which the `content` parameter's
  description now states:
  - content is written at the file's columns;
  - a first line without indentation takes the target's column, and leading whitespace
    on it replaces that line's indentation;
  - continuation lines are kept as written;
  - content written dedented, relative to column 0, is still shifted to the target;
  - lines inside string literals never move.

  An insert next to code that occupies whole lines goes on lines of its own. An insert
  within a line stays inline. Inserted line breaks match the file's line endings.
- `undo_edit` is multi-level. Calling it again reverts a file's previous edit, as long as
  the file hasn't changed in between; the 32 most recent edits across all files are kept.
- **Breaking.** Undo history and tips are kept in the server process rather than in
  session files under `~/.ai-tools`, so they no longer survive a server restart. Paths
  resolve against the server's root rather than a per-session working directory.
- A successful edit's note line names the formatter that ran, or says why none did. The
  first line of every response is unchanged.
- **Breaking, for embedders:**
  - `SemanticEditTools::new(root)` replaces `new(storage_path)` and `embedded(working_directory)`.
    Create one instance per root.
  - Tool calls are async, and their bodies run on tokio's blocking pool, so the host must
    run a tokio runtime. This follows mcplease 0.5.
  - `snapshot()` and `restore()` of a `ToolState` replace `session_snapshot` and
    `restore_session`.
  - The commit hook is set with `with_commit_fn` and must be `Send + Sync`
    (`CommitFn = Arc<dyn Fn(PathBuf, String) + Send + Sync>`).
  - `Editor::new` and `Editor::from_source` take the `Formatting` to use.
  - `LanguageEditor::format_code` and `Editor::format_code` are removed, because
    formatting is no longer part of a language.

### Removed

- **Breaking.** The `set_working_directory` tool. A path is absolute, or relative to the
  server's root (`--root`).

### Fixed

- Python edits whose content was indented the way agents usually write it, at the file's
  columns like the anchor, had their continuation lines pushed a level too deep and were
  refused.
- Re-indenting Python content changed the contents of multi-line strings in it.
- An `insert_before` in nested Python code could land inside the wrong block.
- Inserting after a Python decorator could merge the two into `@a @ b`.
- Inserting a TOML table after another table's header moved that table's keys into the
  new table.
- Deleting a Rust item left its doc comments and attributes behind. They would dangle, or
  attach to the next item (`#[test] #[test] fn next`).
- A replace anchored on a Rust function's signature duplicated its doc comment and
  attributes.
- Replacing a Rust `//` comment, or a line together with its line break, added a blank
  line.
- Deleting whole lines could leave an empty line or doubled blank lines behind.
- Inserts into files with CRLF line endings added LF line breaks, and diffs showed stray
  `\r`.
- A large insert written with different indentation could make biome reindent the whole
  file to match it.
- Very deeply nested files could crash the server with a stack overflow.

## [0.3.0] - 2026-09-13

### Added

- `edit` applies a change and writes it out in a single call, returning the resulting
  diff for review. Omitting `content` deletes the targeted code.
- `undo_edit` restores a file to its exact content from before the most recent edit.
  It is single-level — each new edit replaces what is available to undo — and refuses
  to act if the file has changed on disk since that edit.
- `find_anchor` reports every location an anchor matches, with surrounding source
  context, without changing anything.
- A selector may target a named item — `item: {kind, name}` — instead of `anchor` text.
  The item resolves together with its leading attributes and doc comments, so
  `insert_before` and `insert_after` land outside the whole decorated item rather than
  between an item and its own attributes. `kind` is matched loosely, so `function`,
  `function_item`, and `function_definition` are interchangeable. Supply `anchor` or
  `item`, not both.
- The crate now builds as a library in addition to the binary, for running the tools
  in-process instead of over stdio: `SemanticEditTools` plus the generated `Tools`
  dispatcher are public, `SemanticEditTools::embedded` constructs an instance backed by
  in-memory session stores that never touches `~/.ai-tools`, and `session_snapshot` /
  `restore_session` round-trip the complete per-session tool state so a resumed session
  picks up where it left off.
- After a session has made several successful edits, responses may carry a `===TIP===`
  section demonstrating a shorter anchor that would have produced the identical result.
  Tips stop once the shorthand is in use, and never appear in a short session.

### Changed

- **Breaking.** `preview_edit` and `persist_edit` are gone, and with them the
  preview-then-persist workflow. `edit` validates and writes in one step; review the
  returned diff and use `retarget_edit` or `undo_edit` to recover. Callers that staged a
  preview and committed it separately now make a single `edit` call.
- **Breaking.** A rejected edit is reported as a tool error rather than as a successful
  call whose body happens to describe a failure.
- **Breaking.** `retarget_edit` now moves an already-applied edit instead of adjusting a
  pending one. It re-runs the recorded content against the pre-edit file and only
  rewrites it if the new placement validates, so a failed retarget leaves the previous
  placement intact and still retargetable.
- **Breaking.** The commit hook accepted by `set_commit_fn` must now be `Send`.
- `edit`'s success report shows a focused diff: hunks whose deletions and insertions are
  identical once whitespace is removed are dropped and counted, so whole-file
  reformatting no longer buries the change. Any hunk that changes a token is kept.
  Failure reports still show the full diff.
- Diffs carry `@@` hunk headers.
- `edit` guards against re-sending content that was just applied: an identical resend is
  a no-op, an identical `replace` at a different anchor is refused, and an identical
  insert elsewhere applies but warns that the file now contains both placements.
- When an anchor matches more than one location, the response lists every match with
  source context rather than silently editing the first one unannounced.
- Inserting into a comma-separated list — object literals, arrays, argument lists, enum
  bodies — now picks up the separator in JavaScript, TypeScript, JSX, TSX and JSON,
  where previously only JSON gained a trailing comma and only on one side.

### Fixed

- Rust files containing a local `struct`, `enum`, `impl`, `trait`, `mod`, nested `fn`, or
  an `async fn` in a trait were rejecting every edit, including edits nowhere near that
  code. Those constructs are valid Rust and are no longer flagged; more generally, an
  edit is now only rejected for problems it introduces, so pre-existing code can never
  make a file uneditable.
- TOML edits that produced malformed output were being accepted and written. They are now
  rejected with the file untouched.
- A TOML `replace` whose anchor straddled a table boundary could delete neighboring
  `[table]` headers and their contents. Replacements now prefer the smallest span that
  covers the anchor.
- Inserting a Python statement after another statement appended it to the anchor's line
  instead of placing it on its own line.
- Anchors that did not line up with complete syntax nodes could produce edits split
  mid-node; these are now rejected instead.
- Rust edits keep an item's leading attributes and doc comments attached to it, rather
  than duplicating them, orphaning them, or inserting between an item and its own
  `#[attribute]`.

## [0.2.1](https://github.com/jbr/semantic-edit-mcp/compare/v0.2.0...v0.2.1) - 2025-07-29

### Added

- rust editor grouping improvements

### Other

- Merge pull request #29 from jbr/lockfile
- *(lockfile)* lockfile maintenance
- format
- document external dependencies

## [0.2.0](https://github.com/jbr/semantic-edit-mcp/compare/v0.1.2...v0.2.0) - 2025-07-26

### Added

- simplify "other" editor
- python improvements
- add reindentation to python
- finally a problem-free user testing session!
- return to node-only operations
- remove non-ast operations, fix duplication
- partially towards a simpler system
- invalidate multiline anchors
- sessions are reloaded when needed
- [**breaking**] this commit represents a complete rewrite of this server
- *(validation)* add complete context validation to insert_before_node and wrap_node
- *(validation)* make context validation language-aware
- *(validation)* add syntax validation safety check to prevent file corruption
- *(integration)* wire up new language system with existing tools
- *(languages)* implement Phase 1 - query-based language abstractions

### Fixed

- rust editor semantic validation
- remove two false-positive rust validations
- *(lint)* clippy
- *(lint)* fmt

### Other

- add coverage
- use trusted-publishers workflow
- attempt to fix tests
- user-facing documentation for whitespace-insensitivity
- *(deps)* use released mcplease 0.2
- remove old docs
- tweak inline docs
- tooling documentation iteration
- commit some development documentation
- comprehensive documentation update to reflect current state
- add adding-languages.md

## [0.1.2](https://github.com/jbr/semantic-edit-mcp/compare/v0.1.1...v0.1.2) - 2025-06-07

### Added

- *(validation)* implement tree-sitter native context validation system
- add specialized insertion tools and enhanced error messages
- implement preview-only mode for safe operation testing

### Fixed

- only build on nightly in ci, resolve clippy lints
- clippy and fmt

### Other

- remove examples from git
- update PROJECT_SUMMARY.md to reflect Phase 1 completion

## [0.1.1](https://github.com/jbr/semantic-edit-mcp/compare/v0.1.0...v0.1.1) - 2025-06-06

### Fixed

- add Cargo.lock to repository
- LICENSE
- add lifetimes to find_node_by_position
- build on stable

### Other

- clippy
- add .github
- clippy and fmt
