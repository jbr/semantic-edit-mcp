# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

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
