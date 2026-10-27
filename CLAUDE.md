# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What this is

An MCP (Model Context Protocol) server for **AST-aware code editing**. Instead of line/character edits, edits target tree-sitter AST nodes located by a short text `anchor`; the content is laid out in-process, and the result is re-parsed and checked to guarantee it is still syntactically valid before anything touches disk. Reformatting the whole file with a language-native formatter is optional, and off unless configured.

## Commands

```bash
cargo build                          # build
cargo test                           # run all tests (the snapshot suite + unit tests)
cargo fmt --all -- --check           # formatting check (CI gate)
cargo clippy                         # lint (CI gate)
cargo run -- serve                   # run the MCP stdio server locally
cargo run -- serve --format built-in # ...with every built-in formatter on
```

### Snapshot tests (the main test surface)

Most behavior is covered by data-driven snapshot tests under `tests/snapshots/`, discovered and run by `src/tests/snapshot_runner.rs` via the single `run_snapshot_tests` Rust test. Each test is a directory containing `args.json` (one tool call or an array of them), an optional `input.<ext>`, and the expected `response.txt` / `output.<ext>`.

```bash
TEST_FILTER=rust cargo test                    # run a category (prefix match)
TEST_FILTER=rust::attributes cargo test        # run one test (its full "::"-joined path)
TEST_FILTER=rust::repair cargo test            # run every test beneath a "::" path
TEST_FILTER=json,rust cargo test               # multiple, comma-separated
UPDATE_SNAPSHOTS=1 cargo test                  # regenerate all expected files
UPDATE_SNAPSHOTS=1 TEST_FILTER=rust::attributes cargo test   # regenerate one
```

A pattern without `::` is a plain prefix (`py` matches `python::…`). A `::` path matches the test with that name and every test beneath it, on `::` boundaries: `rust::repair` matches `rust::repair::insert` but not `rust::repairs::…`.

When a snapshot fails, the summary prints the exact `TEST_FILTER=...` and `UPDATE_SNAPSHOTS=1 ...` commands to reproduce or accept it. Always review a diff before accepting it with `UPDATE_SNAPSHOTS=1` — the whole point of the suite is to catch unintended formatting/targeting changes.

Tests start from the product default, formatting off, and a `formatting.json` changes that. One in any directory under `tests/snapshots/` (the root and a test's own directory included) applies to every test beneath it; nested files layer outermost first, each overriding the last. It maps a language, named as in `args.json` (`rust`, `typescript`, …), or `"*"` for every language, to a formatter (`"rustfmt"`, `"ruff"`, `"biome"`, `"taplo"`) or `null` for none; `"*"` applies before the languages beside it. `{"*": null}` is formatting off everywhere, `{"rust": null}` off for Rust alone. The root `tests/snapshots/formatting.json` turns on the built-in set (`Formatting::built_in`, which a unit test holds it to) for the whole tree, since its expectations were written with formatting on; everything under `tests/snapshots/unformatted/` runs with formatting off again. So a new test is formatted unless it goes under `unformatted/` — put it there to test layout and validation on their own. A `Formatter::command` has no `formatting.json` form; the tests of command formatters (the per-file failure rule, the veto) are in `tests/embedding.rs`, with shell scripts as formatters.

## External formatter dependencies

Formatting is off by default, so editing needs nothing outside the process. Which formatter, if any, runs for each language is configuration: a `Formatting` (`src/formatting.rs`) set on `SemanticEditTools` with `with_formatting`, or replaced on a live instance with `set_formatting`. `Formatting::off()` is the default, and `Formatting::built_in()` the built-in set below; a built-in `Formatter` parses from its name (`"rustfmt"`, …), and its `Display` is that name, or the program for `Formatter::command(program, args)`, which runs any program that reads the source on stdin and writes the result to stdout, with `{path}` in an argument replaced by the file's path. The standalone binary builds its settings from `--format LANGUAGE=FORMATTER` (repeatable, applied in turn) and `--format built-in`, before or after the subcommand; it can't configure a command. With no formatter for a language its edits are written as laid out.

The runner (`src/formatting/command.rs`) runs a program the way its project would: in the file's directory, so rustfmt finds `rustfmt.toml` and biome `biome.json`, with the file's path in the tool's own flag (ruff `--stdin-filename`, with `--force-exclude` so the project's exclusions hold; biome `--stdin-file-path`), and with biome's `--indent-style`/`--indent-width` (the pre-edit file's indentation) only when no `biome.json(c)`/`.biome.json(c)` is in the file's ancestors and `BIOME_CONFIG_PATH` is unset. stdin is written on its own thread while stdout and stderr are drained on theirs, and a run has `TIMEOUT` (10s), after which the program is killed. Programs are resolved by `src/formatting/resolve.rs` as the platform does, including Windows `PATHEXT` (npm's `.cmd` shims); its search takes `PATH`, `PATHEXT` and the file check as values, so the Windows rules are unit-tested everywhere.

A formatter's failure is judged per file (`src/editor/format.rs`, `Editor::format`): one that can't run (not found, not executable, timed out) is absent, and the laid-out result is written with a note saying why; one that fails, or whose output fails the same validation as the edit, is run on the pre-edit file, and the edit is rejected only if that succeeds (`The formatter has encountered …`, a first line efference's corpus script keys on), else it is absent for this edit with a note quoting its error. In repair mode a failure leaves the repair unformatted with its warning. The built-in set:

- **Rust** → `rustfmt`, at the edition the nearest `Cargo.toml` declares (2024 when none does; `src/formatting/rustfmt.rs`)
- **Python** → `ruff`
- **JS / TS / JSX / TSX / JSON** → `biome`
- **TOML** → `taplo`, in-process (nothing to install)
- **plain / markdown / unknown** → none

The snapshot suite runs with the built-in set (see above), so `cargo test` needs `rustfmt`, `ruff` and `biome` on `PATH`; CI installs them.

The design for making formatting optional is `docs/design/formatting.md`.

## Architecture

### The edit pipeline

The core flow lives in `src/editor.rs` (`Editor`) and `src/editor/edit.rs` (`Edit`):

1. **Locate candidates.** `EditIterator` (`src/editor/edit_iterator.rs`) turns a `Selector` (operation + anchor) into an *ordered list of candidate edits*. `searcher::find_positions` finds the anchor whitespace-insensitively; for each hit it produces several candidates at different AST granularities (exact byte range, the sibling nodes in range, the common parent) plus, for inserts, whitespace variants. This is a **try-in-order** strategy, not a single deterministic target.
2. **Lay out each candidate** (`layout::lay_out`, called from `Editor::build_edits`; see `src/layout.rs` and `docs/design/formatting.md`, "Layout"), formatter or not. The content contract: content is in file columns, a bare first line takes the target's column, leading whitespace on the first line replaces that indentation, continuation lines are kept unless the content is evidently dedented, and a line inside a string literal never moves. The seams: an insert next to code occupying whole lines goes on lines of its own at that code's column (also past a trailing `,`/`;` or comment), one inside a line stays inline, blank lines come only from the content, inserted line breaks take the file's line ending, and a whole-line delete takes its line with it.
3. **Apply + validate each candidate** until one succeeds (`Edit::apply`). Applying edits a `ropey::Rope` + incremental tree-sitter `Tree`, then re-parses. A candidate is rejected if the result fails to parse, contains tree-sitter ERROR/MISSING nodes (`LanguageEditor::collect_errors`), introduces a context-validation violation, or fails the language's in-process `check` (delta-gated; see below). The first candidate that survives all checks wins. Validation alone picks it: the formatter plays no part in choosing.
4. **Format the whole file** with the language's formatter, when the settings give it one (`Editor::format`, `src/editor/format.rs`); without one, the laid-out result is written as is. The formatter runs once, on the winning candidate only, and its failure is judged per file (see "External formatter dependencies"): it may **veto** the edit, but never moves on to another candidate. The success report's first line says what happened to the whitespace: `Note: rustfmt formatted the whole file, including the edit.`, a note on why it wasn't formatted, or that no formatter ran. The entire file is reformatted, not just the edited region, so the file-level diff can carry formatter churn arbitrarily far from the edit. The success report of a formatted edit therefore shows a **focused diff** (`editor::focused_diff`): hunks whose deletions and insertions are identical once whitespace is removed are dropped and counted, everything else is kept wherever it sits — a formatter that changes a *token* is a semantic difference from the caller's model and always stays. Unformatted, every hunk is the edit's own, so none is dropped. Failure reports still show the full `clean_diff`. Both carry `@@` hunk headers, which are the result's only line numbers.
5. **Report + record.** `Editor::apply` runs the edit against the in-memory source and, on success, returns the diff report plus an `AppliedEdit` record — it never writes; the calling *tool* persists the record's `post_edit_source` (via `SemanticEditTools::persist_output`) and stores the record for undo.

### Tools and the optimistic-persist workflow (`src/tools/`)

Tools are registered through the `mcplease::tools!` macro in `src/tools.rs`. The workflow is **edit → review the diff → (if mistargeted) retarget or undo**; a validated edit is written immediately, so recovery is first-class:

- `edit` — apply an edit; if it validates it is persisted at once and the diff is returned for review. Omitting `content` means *delete*. Because persistence is immediate, `edit` guards against re-sends of the just-applied content: an identical resend is a no-op, an identical `replace` at a different anchor is refused (it would duplicate the content *and* overwrite the undo record), and an identical insert elsewhere applies but warns.
- `retarget_edit` — move the last edit to a corrected anchor in one step: it re-runs the edit's content against the recorded *pre-edit* source and only rewrites the file if the new placement validates, so a failed retarget leaves the previous placement applied.
- `undo_edit` — restore the file's exact content from before its most recent edit. Calling it again reverts the edit before that, as long as the file hasn't changed in between.
- `find_anchor` — report every location an anchor matches, without editing.

A selector normally carries `anchor` text. It may instead carry `item` — `{kind, name}` (`src/item.rs`) — which resolves a named item **together with its leading outer attributes and doc comments**, so `insert_before`/`insert_after` land outside that whole decorated unit rather than between an item and its own `#[test]`. `kind` is matched loosely against the grammar's node kinds (`function` ≡ `function_item` ≡ `function_definition`). Exactly one of `anchor`/`item` is required; the result names the resolved item, its line span, and the neighbor on the side the content went. Textual anchors remain the general path — item targeting is not a query language and should not grow into one without evidence from further repeated targeting failures.

A `file_path` is absolute, or relative to the root the tools were created with (`SemanticEditTools::new(root)`; the binary's `--root`, defaulting to its working directory). There is no tool for changing it.

`AppliedEdit` (in `src/state.rs`) is the serializable undo/retarget record: the `EditOperation` (selector + content + file + language) plus the full pre- and post-edit file content. Storing both sides makes undo a byte-exact restore (no reverse-diff application) and lets undo/retarget verify the file on disk still matches `post_edit_source` before acting — a stale record refuses rather than clobbering outside changes. That freshness check is skipped when a `commit_fn` is injected, because then the embedder owns persistence and disk isn't observable.

### State (`src/state.rs`)

`SemanticEditTools` is the shared tool state, cloned into every call. Its memory is a `ToolState` held in-process (nothing is written outside the edited files): the most recent applied edits across all files, at most `EDIT_HISTORY_LIMIT`, found by the file they were made to, plus the education state. An embedder that keeps its own log can `snapshot()` it and `restore()` it on resume. Every call that writes holds `write_lock` throughout, since an edit is a read-modify-write of a file. `commit_fn` is an injectable write hook — `None` means the tool writes to disk itself; embedders set it to capture output instead. It also holds the `Formatting` settings, shared with its clones so that `set_formatting` reaches a live instance; each tool call takes one snapshot (`formatting()`) and builds every `Editor` from it, and the `Editor` keeps its file's `Formatter`. The snapshot runner does **not** use `commit_fn`: it copies each test's input into a scratch dir under `target/snapshot-scratch/` and lets edits write for real, so sequential edits accumulate and undo/retarget freshness checks are exercised (`output.<ext>` is the scratch file's final content, omitted when the calls left no net change).

### Languages (`src/languages/`)

`LanguageRegistry` maps `LanguageName` (an `enum_map` key) → `LanguageCommon` (tree-sitter `Language`, file extensions, a `Box<dyn LanguageEditor>`, and an optional validation `Query`). Language detection is by file extension, overridable with an explicit hint.

`LanguageEditor` (`src/languages/traits.rs`) is the per-language extension point; all methods have working defaults, so a language can be just `language()` returning a `LanguageCommon`. Override points:

- `build_edits` — customize candidate generation. **Rust** (`rust.rs`) is the most complex: after the generic candidates it does *node grouping* — expanding a selection backward/forward so leading comments and `#[attribute]`s stay attached to the item they decorate, preventing duplication/orphaning on insert and replace.
- `layout` — the `layout::Rules` content is laid out by: the string-literal node kinds whose lines never move, which readings of the content's columns to try (Python tries both and lets validation choose; other code only the evidence's; plain/Markdown always takes content as written), Python's statement containers, and whether the file is prose.
- `collect_errors` — default walks the tree for ERROR/MISSING nodes.
- `check` — a stricter in-process validity check, standing in for the compiler-grade parse a formatter used to provide: `syn::parse_file` for Rust, the language reference's indentation rules for Python (`languages/python/indentation.rs`), taplo's DOM validation for TOML; the default accepts everything. Delta-gated like the context queries (`Editor::passes_check` caches the pre-edit result), so it only blocks edits to a file that passed it before. When it was off for an edit and the result still fails it, the success report adds a note with the check's diagnostic for the result (`Editor::apply`), so the caller knows the check didn't guard that edit.

The four ECMAScript-family languages (js/ts/jsx/tsx) share `EcmaEditor` (`ecma_editor.rs`).

Formatting is not a language concern: a `Formatter` (`src/formatting.rs`) carries its own invocation, and which one a language gets is the `Formatting` settings. A language's settings are looked up by `LanguageCommon::name`, so it must equal the language's registry key.

### Context-validation queries (`queries/<lang>/validation.scm`)

Beyond "does it parse", languages can ship tree-sitter queries that flag semantically-invalid-but-parseable edits. `ContextValidator` (`src/validation/context_validator.rs`) runs them. Candidate validation is **delta-gated** (`Editor::validate_tree`): violations are counted against the original file, and a candidate only fails for violations the edit *introduces* — a query match on pre-existing code must never make a file uneditable. Rules must still only flag genuinely invalid code (see the removed-rule NOTEs in `queries/rust/validation.scm` for the false-positive history). Two important subtleties it handles manually because tree-sitter does **not**: standard text predicates (`#eq?`, `#match?`, `#any-of?`) must be applied via `satisfies_text_predicates`, and the **custom** `#has-ancestor?` / `#not-has-ancestor?` predicates are evaluated by hand (`satisfies_ancestor_predicates`). Captures named `invalid.*` become violations. If you add a query rule that over- or under-fires, check these two predicate paths first.

### Binary vs. library

`src/main.rs` is the standalone stdio MCP server (`mcplease::calls`, each call spawned on tokio). Its `--format` option isn't clap-`global`, because a global option keeps only the occurrences on one side of the subcommand; `Args::parse_argv` adds it to each subcommand and joins both sides, so parse with that rather than clap's `Parser` methods. `src/lib.rs` re-exposes the same modules for **in-process embedding**: a host builds one `SemanticEditTools::new(root)` per agent and runs tools through the generated `Tools` dispatcher; tool bodies run on tokio's blocking pool, so the host must be on a tokio runtime. Keep additions reachable from both — `main.rs` uses `#[deny(dead_code)]`, while `lib.rs` allows it (items reachable only from the binary look dead in the library view).

## Conventions

- Commits follow **Conventional Commits** (enforced by CI via `convco`); releases are automated by release-plz.
- Edition 2024.
- The crate makes heavy use of the `fieldwork` derive for accessors — prefer adding a `#[fieldwork]` attribute over hand-writing getters/setters to match surrounding style.
