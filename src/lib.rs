//! Library surface for **in-process embedding** (e.g. the efference harness).
//!
//! The binary (`main.rs`) remains the standalone MCP stdio server. This lib
//! re-exposes the same modules so a host can run a tool directly via the
//! generated [`tools::Tools`] dispatcher + [`state::SemanticEditTools`], without
//! `mcplease::run`'s stdio/clap/logger ownership. Use
//! [`SemanticEditTools::embedded`] to construct with in-memory session stores so
//! embedding never touches the shared `~/.ai-tools` store.
//!
//! Additive only: `main.rs` and `tests` keep declaring their own modules, so the
//! binary and its tests are unchanged. Deleting this file reverts the crate to
//! bin-only. `dead_code` is allowed here because items reachable only from the
//! binary or its tests would otherwise read as dead in the library view.

#![allow(clippy::collapsible_if)]
#![allow(dead_code)]

pub mod editor;
pub mod education;
pub mod indentation;
pub mod item;
pub mod languages;
pub mod searcher;
pub mod selector;
pub mod state;
pub mod tools;
pub mod validation;

pub use state::SemanticEditTools;
pub use tools::Tools;
