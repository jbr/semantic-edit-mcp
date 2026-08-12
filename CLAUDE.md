# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

An MCP (Model Context Protocol) server for **AST-aware code editing**. Instead of line/character edits, edits target tree-sitter AST nodes located by a short text `anchor`, then the whole file is reformatted with the language's native formatter and re-parsed to guarantee the result is still syntactically valid before anything touches disk.

## Commands

```bash
cargo build                          # build
cargo test                           # run all tests (the snapshot suite + unit tests)
cargo fmt --all -- --check           # formatting check (CI gate)
cargo clippy                         # lint (CI gate)
cargo run -- serve                   # run the MCP stdio server locally
```

### Snapshot tests (the main test surface)

Most behavior is covered by data-driven snapshot tests under `tests/snapshots/`, discovered and run by `src/tests/snapshot_runner.rs` via the single `run_snapshot_tests` Rust test. Each test is a directory containing `args.json` (one tool call or an array of them), an optional `input.<ext>`, and the expected `response.txt` / `output.<ext>`.

```bash
TEST_FILTER=rust cargo test                    # run a category (prefix match)
TEST_FILTER=rust::attributes cargo test        # run one test (exact, "::"-joined path)
TEST_FILTER=json,rust cargo test               # multiple, comma-separated
UPDATE_SNAPSHOTS=1 cargo test                  # regenerate all expected files
UPDATE_SNAPSHOTS=1 TEST_FILTER=rust::attributes cargo test   # regenerate one
```

When a snapshot fails, the summary prints the exact `TEST_FILTER=...` and `UPDATE_SNAPSHOTS=1 ...` commands to reproduce or accept it. Always review a diff before accepting it with `UPDATE_SNAPSHOTS=1` — the whole point of the suite is to catch unintended formatting/targeting changes.

## External formatter dependencies

Editing relies on language-native formatters being on `PATH`; an edit fails (and is reported as unsafe) if its formatter is missing or errors:

- **Rust** → `rustfmt` (currently hardcoded to `--edition 2024` in `src/languages/rust.rs`)
- **Python** → `ruff`
- **JS / TS / JSX / TSX / JSON** → `biome`
- **TOML** → none (uses the `taplo` crate in-process)
- **plain / markdown / unknown** → none

## Architecture

### The edit pipeline

The core flow lives in `src/editor.rs` (`Editor`) and `src/editor/edit.rs` (`Edit`):

1. **Locate candidates.** `EditIterator` (`src/editor/edit_iterator.rs`) turns a `Selector` (operation + anchor) into an *ordered list of candidate edits*. `searcher::find_positions` finds the anchor whitespace-insensitively; for each hit it produces several candidates at different AST granularities (exact byte range, the sibling nodes in range, the common parent) plus, for inserts, whitespace variants. This is a **try-in-order** strategy, not a single deterministic target.
2. **Apply + validate each candidate** until one succeeds (`Edit::apply`). Applying edits a `ropey::Rope` + incremental tree-sitter `Tree`, then re-parses. A candidate is rejected if the result fails to parse, contains tree-sitter ERROR/MISSING nodes (`LanguageEditor::collect_errors`), fails the language's context-validation query, or the formatter errors. The first candidate that survives all checks wins.
3. **Format the whole file** with the language formatter. Note: the entire file is reformatted, not just the edited region — diffs may show formatting changes beyond your edit.
4. **Report + record.** `Editor::apply` runs the edit against the in-memory source and, on success, returns the diff report plus an `AppliedEdit` record — it never writes; the calling *tool* persists the record's `post_edit_source` (via `SemanticEditTools::persist_output`) and stores the record for undo.

### Tools and the optimistic-persist workflow (`src/tools/`)

Tools are registered through the `mcplease::tools!` macro in `src/tools.rs`. The workflow is **edit → review the diff → (if mistargeted) retarget or undo**; a validated edit is written immediately, so recovery is first-class:

- `edit` — apply an edit; if it validates it is persisted at once and the diff is returned for review. Omitting `content` means *delete*. Because persistence is immediate, `edit` guards against re-sends of the just-applied content: an identical resend is a no-op, an identical `replace` at a different anchor is refused (it would duplicate the content *and* overwrite the undo record), and an identical insert elsewhere applies but warns.
- `retarget_edit` — move the last edit to a corrected anchor in one step: it re-runs the edit's content against the recorded *pre-edit* source and only rewrites the file if the new placement validates, so a failed retarget leaves the previous placement applied.
- `undo_edit` — restore the file's exact pre-edit content. Single-level: each edit replaces the undo record.
- `find_anchor` — report every location an anchor matches, without editing.
- `set_working_directory` — set the session's root so later `file_path`s can be relative.

`AppliedEdit` (in `src/state.rs`) is the serializable undo/retarget record: the `EditOperation` (selector + content + file + language) plus the full pre- and post-edit file content. Storing both sides makes undo a byte-exact restore (no reverse-diff application) and lets undo/retarget verify the file on disk still matches `post_edit_source` before acting — a stale record refuses rather than clobbering outside changes. That freshness check is skipped when a `commit_fn` is injected, because then the embedder owns persistence and disk isn't observable.

### State and sessions (`src/state.rs`)

`SemanticEditTools` is the shared tool state. It holds two `mcplease` `SessionStore`s: a **private** one for the undo record and education state (path from `MCP_SESSION_STORAGE_PATH`, default `~/.ai-tools/sessions/semantic-edit.json` — so the undo record survives a server restart) and a **shared** cross-server one at `~/.ai-tools/sessions/shared-context.json` for the working directory. `commit_fn` is an injectable write hook — `None` means the tool writes to disk itself; embedders set it to capture output instead. The snapshot runner does **not** use `commit_fn`: it copies each test's input into a scratch dir under `target/snapshot-scratch/` and lets edits write for real, so sequential edits accumulate and undo/retarget freshness checks are exercised (`output.<ext>` is the scratch file's final content, omitted when the calls left no net change).

### Languages (`src/languages/`)

`LanguageRegistry` maps `LanguageName` (an `enum_map` key) → `LanguageCommon` (tree-sitter `Language`, file extensions, a `Box<dyn LanguageEditor>`, and an optional validation `Query`). Language detection is by file extension, overridable with an explicit hint.

`LanguageEditor` (`src/languages/traits.rs`) is the per-language extension point; all methods have working defaults, so a language can be just `language()` returning a `LanguageCommon`. Override points:

- `format_code` — shell out to the formatter (see `ecma_editor.rs`, `rust.rs`, `python.rs`).
- `build_edits` — customize candidate generation. **Rust** (`rust.rs`) is the most complex: after the generic candidates it does *node grouping* — expanding a selection backward/forward so leading comments and `#[attribute]`s stay attached to the item they decorate, preventing duplication/orphaning on insert and replace.
- `collect_errors` — default walks the tree for ERROR/MISSING nodes.

The four ECMAScript-family languages (js/ts/jsx/tsx) share `EcmaEditor` (`ecma_editor.rs`).

### Context-validation queries (`queries/<lang>/validation.scm`)

Beyond "does it parse", languages can ship tree-sitter queries that flag semantically-invalid-but-parseable edits. `ContextValidator` (`src/validation/context_validator.rs`) runs them. Candidate validation is **delta-gated** (`Editor::validate_tree`): violations are counted against the original file, and a candidate only fails for violations the edit *introduces* — a query match on pre-existing code must never make a file uneditable. Rules must still only flag genuinely invalid code (see the removed-rule NOTEs in `queries/rust/validation.scm` for the false-positive history). Two important subtleties it handles manually because tree-sitter does **not**: standard text predicates (`#eq?`, `#match?`, `#any-of?`) must be applied via `satisfies_text_predicates`, and the **custom** `#has-ancestor?` / `#not-has-ancestor?` predicates are evaluated by hand (`satisfies_ancestor_predicates`). Captures named `invalid.*` become violations. If you add a query rule that over- or under-fires, check these two predicate paths first.

### Binary vs. library

`src/main.rs` is the standalone stdio MCP server (`mcplease::run`). `src/lib.rs` re-exposes the same modules for **in-process embedding** (e.g. running a tool directly against `SemanticEditTools::embedded`, which uses in-memory session stores and never touches `~/.ai-tools`). Keep additions reachable from both — `main.rs` uses `#[deny(dead_code)]`, while `lib.rs` allows it (items reachable only from the binary look dead in the library view).

## Conventions

- Commits follow **Conventional Commits** (enforced by CI via `convco`); releases are automated by release-plz.
- Edition 2024.
- The crate makes heavy use of the `fieldwork` derive for accessors — prefer adding a `#[fieldwork]` attribute over hand-writing getters/setters to match surrounding style.
