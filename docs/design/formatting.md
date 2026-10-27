# Formatting, layout and validation

semantic-edit began as an opinionated tool. Every edit reformatted the whole file with one
formatter per language, and an edit failed if that formatter was missing or refused the
result. Embedded in a general-purpose harness (efference) that has to run on any machine and
in any project, both properties are wrong: a missing formatter must not make a language
uneditable, and the tool must not restyle a project that doesn't use its formatter.

This is the design for making formatting optional without losing what the formatter was
quietly providing.

## What the formatter was doing

Making formatting optional means replacing everything it did, and it did more than format.

1. **Formatted the agent's content.** The intended job.
2. **Reformatted the whole file.** Run the way semantic-edit runs them, rustfmt and biome
   rewrite CRLF line endings to LF. On files that weren't already formatter-clean, applied
   edits carried whitespace-only changes elsewhere in the file: 3% of Rust edits, 3–4% of
   TypeScript and TSX edits, 23% of Python edits. The focused diff hid those hunks from the
   agent while still writing them.
3. **Repaired the whitespace where content was spliced in.** Candidate generation emits
   splices that the formatter tidied: inserts glued onto the anchor's line
   (`name: string;email: string;`), and a replaced item's first line indented twice while the
   next line lost its indentation. With formatting stubbed out, 11 of the 23 snapshot tests
   that changed did so for this reason.
4. **Validated.** The formatters parse with compiler-grade parsers. tree-sitter's grammars are
   built for editors: they accept more than the language and recover from errors. Across
   4,228 logged `edit` calls, the formatter refused 10 results that tree-sitter accepted.
5. **Steered targeting.** A refusal removes a candidate, so whether a formatter was installed
   changed which candidate won.
6. **Applied the wrong project's configuration.** Fed on stdin without the file's path, rustfmt
   and ruff read configuration from the server process's working directory, not from the
   file's project. biome did the same, and the `--indent-style`/`--indent-width` flags it was
   given overrode the project's own `biome.json`.

Of the 10 refusals, 7 were Python, and in each of those the tool, not the agent, produced the
invalid indentation. The agent wrote correctly indented content, and the Python reindent
moved it (see [Layout](#layout)). So the formatter was mostly catching semantic-edit's own
bugs. Of the other three, one was an agent error (a `struct` inside an `impl`). The other two
were deletions: one left a doc comment with no item after it, and one left TSX that biome
couldn't parse.

## Principles

1. **Correctness never depends on an external program.** Layout and validation run
   in-process, so the guarantees hold on any machine.
2. **Edits are local.** Bytes outside the edit change only if the embedder asks for whole-file
   formatting.
3. **Running a program is the embedder's decision.** semantic-edit never decides on its own to
   execute a formatter. Which one runs, if any, is configuration.
4. **Formatting may veto an edit but never move it.** Candidate selection uses in-process
   validation only.
5. **Each response says what happened to the content's whitespace:** formatted (and by what),
   or written as given.
6. **A formatter that runs is invoked the way its project would invoke it.**

## The pipeline

targeting → layout → validation → formatting (optional) → report

### Layout

Layout decides the whitespace at the seams of a splice and the indentation of the content. It
is always on and needs nothing outside the process.

**The content contract.** Agents write `content` the way they copy anchors: at the file's
columns, often leaving off the first line's indentation, because an anchor's first line
matches from its first non-whitespace character. Among logged multi-line `replace` calls on
indented targets, content was at file columns in 99% of Rust calls, 97% of TSX, 90% of Python
and 88% of TypeScript. Content dedented to column 0 was the exception. So:

- Content is in file columns. A first line without leading whitespace is placed at the
  target's column. Leading whitespace on the first line replaces the target line's
  indentation.
- Continuation lines are kept as written.
- Content is rebased only when it is evidently dedented, meaning its continuation lines sit
  left of the target's column. Where indentation is syntax (Python), a dedented reading is
  also offered as a later candidate, and validation chooses between the two readings.
- A line that begins inside a string literal is never re-indented.
- The `content` parameter's description states this contract, so an agent reads it before its
  first edit rather than learning it from a refusal.

The old Python reindent got this backwards. It read continuation lines as relative to the
first line and shifted them by the target's indentation, which caused the Python refusals
above.

**Seams.**

- An insert next to an item that occupies whole lines goes on its own lines at that item's
  column. That includes the case where a trailing separator (`,` or `;`) ends the anchor's
  line. An insert inside a line, such as a parameter or an argument, stays inline.
- Blank lines between the new code and its neighbors come from the content, as the tool
  already documents.
- Inserted text takes the file's line endings.

### Validation

A candidate is accepted only if it parses with no tree-sitter ERROR or MISSING nodes and
introduces no context-validation violation, as before. Because formatting is optional, a
stricter in-process check per language takes over the formatter's validating role.

| Language | Check | Why this one |
|---|---|---|
| Rust | `syn::parse_file` | It rejects the logged cases tree-sitter accepts: an item an `impl` can't hold, and a doc comment with nothing after it. It accepted every recent stable construct tried and all 1,659 files of five real codebases. It is already in the dependency graph. |
| Python | The language reference's indentation rules, checked over the result's logical lines | Every Python refusal in the logs was indentation, and those rules haven't changed since Python 3. A full parser would lag the language: rustpython-parser 0.4.0 rejects valid 3.12–3.14 syntax, and `ruff_python_parser` is published only as an unstable internal crate. |
| TOML | taplo's DOM validation | It catches duplicate keys, which tree-sitter can't see and which semantic-edit currently writes. It is already a dependency. |
| JS/TS/JSX/TSX | none yet | There was one refusal in 445 logged edits. Revisit with `oxc_parser` if more appear. |

Each check is **delta-gated**, like the context queries: it blocks an edit only if the file
passed it before the edit. A file the checker can't handle, because of newer syntax or nightly
features, stays editable. The remaining risk is an agent introducing syntax the checker doesn't
know into a file that didn't already use it. That is why a check ships only after its false
rejections have been measured on known-valid code (current files, their git history, the
Python standard library) and its catches on logged refusals.

Unlike formatting, validation steers: it decides which candidate wins.

### Formatting

Formatting is off unless the embedder configures a formatter for the language.

- **Configured with builder types, not a config schema.** The embedder decides how its users
  spell the setting.
  - Named constructors, for example `Formatter::rustfmt()`, carry each tool's correct
    invocation.
  - A command formatter runs anything that reads the source on stdin and writes the result to
    stdout, with the file's path available as `{path}`.
  - Settings can change on a live instance.
  - The standalone server takes the same settings on its command line.
- **Invocation.**
  - The working directory is the file's directory.
  - The file's path is passed in the tool's own flag (`--stdin-filename`, `--stdin-file-path`,
    `--stdin-filepath`).
  - No flags override the project's configuration.
  - stdin is written while stdout and stderr are drained, with a timeout.
  - Executables are resolved the way the platform resolves them, including Windows `PATHEXT`,
    so npm's `.cmd` shims are found.
- **Scope is the edit.** The whole file is formatted so the formatter sees context, but only
  the hunks that touch the edited lines, or the lines on either side of them, are kept. The
  result is then validated again. darker (for black), `git-clang-format` and VS Code's
  `formatOnSaveMode: "modifications"` work the same way. Formatters' own range options are
  too uneven to rely on: ruff and prettier have them, rustfmt's needs nightly, and biome has
  none. Whole-file scope is an explicit option.
- **Failure is judged per file.**

  | The formatter | Treated as |
  |---|---|
  | can't run (missing, or a broken install) | absent; each response notes it |
  | fails on the file as it was before the edit | absent for that file; the note quotes its error |
  | succeeds before the edit and fails after | caused by the edit, which is rejected |
  | succeeds, but its output fails validation | failed, judged by the same rule |

  The pre-edit run happens only when formatting the edit fails. After a formatted edit, the
  next edit's starting point is already known to format.
- **Veto, not steer.** Formatting runs on the winning candidate only. A rejection rejects the
  edit; it doesn't move on to the next candidate. A case where the formatter would have
  steered to a better candidate is a validation gap, and gets fixed in validation.

### Reporting

Each successful edit says, in one line, what happened to its whitespace: formatted (and by
what), or written as given at the target's indentation. Tool descriptions don't promise
formatting. The focused diff matters only under whole-file scope.

## Not in scope

- **Detecting a project's formatter** from its config files, its editor settings, or whether a
  file is already formatter-clean. efference has no way yet to show a detection to a person
  for approval, and agents write its per-project configuration anyway. If detection returns,
  its safe basis is a per-file check: is the file already exactly what this formatter
  produces?
- **Project-local formatter binaries** (`node_modules/.bin`, a virtualenv's `bin`). They are
  the project's own code, so they run only as an explicitly configured command.
- **`.editorconfig`.** The content contract takes indentation from the target line itself, so
  nothing needs the indent unit yet. `.editorconfig` becomes relevant if layout ever converts
  between tabs and spaces.

## Compatibility and rollout

Making formatting off by default is a breaking change: users of the standalone server who
relied on it must configure it. The default can flip only once layout and validation stand on
their own. Before that, turning formatting off would write badly indented Rust and TypeScript,
and invalid Python.

## Evidence

The numbers above come from three sources:

- 4,228 `edit` calls in efference sessions between June and October 2026, by a dozen models,
  mostly on Rust (68%), Markdown (18%) and TypeScript/TSX (10%).
- This crate's snapshot suite, run with formatting stubbed out.
- syn and rustpython-parser, run over related codebases and the Python 3.12 standard library.
