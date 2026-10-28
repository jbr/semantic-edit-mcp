//! Library surface for **in-process embedding** (e.g. the efference harness).
//!
//! The binary (`main.rs`) remains the standalone MCP stdio server. This lib
//! re-exposes the same modules so a host can run a tool directly via the
//! generated [`tools::Tools`] dispatcher + [`state::SemanticEditTools`], without
//! the binary's stdio, clap, or logger. Construct one [`SemanticEditTools::new`]
//! per agent, rooted at its directory; tool bodies run on tokio's blocking
//! pool, so the host must be on a tokio runtime. Edits aren't formatted unless
//! the tools are built [`with_formatting`](SemanticEditTools::with_formatting),
//! for example with [`Formatting::built_in`] or a language formatted by a
//! [`Formatter::command`], and
//! [`set_formatting`](SemanticEditTools::set_formatting) replaces the settings
//! while they run. A formatter's changes are kept to the edit, and to the
//! edit's own text where the formatter moves it, unless the settings ask for
//! the [whole file](Formatting::whole_file).
//!
//! The settings can live in the host's own configuration: [`Formatting`]
//! implements serde's `Deserialize` and `Serialize` and schemars' `JsonSchema`,
//! as a table of each language's formatter, which in TOML reads:
//!
//! ```toml
//! [formatting]
//! whole-file = false
//! rust = "rustfmt"
//! python = { command = "uvx", args = ["ruff", "format", "--stdin-filename", "{path}", "-"] }
//! ```
//!
//! A language's value is a built-in formatter's name, a command's table, or
//! `false` for none, and `built-in = true` gives the languages the table leaves
//! out their built-in formatters; [`FormattingLayer`] describes the shape.
//! Deserializing is strict, so a misspelled key is an error that names the
//! keys expected. For settings read from more than one place, such as a user's
//! configuration and then a project's, read each as a [`FormattingLayer`] and
//! apply them in turn with [`Formatting::with_layer`], so a later one changes
//! only the keys it sets.
//!
//! A host whose configuration type derives `JsonSchema` takes the settings'
//! schema along with no further step: a field of type [`Formatting`] becomes a
//! `$ref` to a `Formatting` definition, which joins the host's own under
//! `$defs` with the definitions it refers to. Its descriptions say what each key
//! does for someone editing the configuration and never name this crate, so
//! they read correctly under the host's name, and the type's own description is
//! the field's: like any `$ref`, the field is best left without a doc comment
//! of its own, which some editors would show in place of the definition's.
//! [`Formatting::schema`] is the same schema on its own, as a root schema.
//!
//! Additive only: `main.rs` and `tests` keep declaring their own modules, so the
//! binary and its tests are unchanged. Deleting this file reverts the crate to
//! bin-only. `dead_code` is allowed here because items reachable only from the
//! binary or its tests would otherwise read as dead in the library view.

#![allow(clippy::collapsible_if)]
#![allow(dead_code)]

pub mod editor;
pub mod education;
pub mod formatting;
pub mod indentation;
pub mod item;
pub mod languages;
pub mod layout;
pub mod searcher;
pub mod selector;
pub mod state;
pub mod tools;
pub mod validation;

pub use formatting::{Formatter, Formatting, FormattingLayer};
pub use languages::LanguageName;
pub use state::SemanticEditTools;
pub use tools::Tools;
