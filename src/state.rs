use crate::{
    education::EducationState,
    formatting::Formatting,
    languages::{LanguageName, LanguageRegistry},
    selector::Selector,
};
use anyhow::{Result, anyhow};
use fieldwork::Fieldwork;
use serde::{Deserialize, Serialize};
use std::{
    collections::VecDeque,
    fmt::{self, Debug, Formatter},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, MutexGuard},
};

/// How many applied edits are held for undo and retarget, across all files.
/// Each holds the whole file on both sides of the edit, so this bounds memory.
pub const EDIT_HISTORY_LIMIT: usize = 32;

/// Persists edited file content in place of writing it to disk, for an
/// embedder that owns persistence.
pub type CommitFn = Arc<dyn Fn(PathBuf, String) + Send + Sync>;

/// Everything these tools remember between calls, as plain serializable data
/// so an embedder that keeps its own log (e.g. efference) can
/// [`snapshot`](SemanticEditTools::snapshot) it and
/// [`restore`](SemanticEditTools::restore) it on resume.
///
/// None of it is keyed by caller. MCP has no sessions, so edits are found by
/// the file they were made to, and the file's contents are what make acting
/// on one safe: an edit is undone or retargeted only while the file still
/// holds exactly what it wrote.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ToolState {
    /// Recently applied edits, oldest first, at most [`EDIT_HISTORY_LIMIT`].
    #[serde(default)]
    edits: VecDeque<AppliedEdit>,
    /// Teaching progress (see [`crate::education`]), per instance of these
    /// tools: per process on stdio, per agent when embedded.
    #[serde(default)]
    education: EducationState,
}

/// What an edit does: the targeting ([`Selector`]), the replacement content,
/// and the file and language it applies to.
#[derive(Debug, Clone, Fieldwork, Serialize, Deserialize, PartialEq, Eq)]
#[fieldwork(get, set, get_mut, with)]
pub struct EditOperation {
    pub selector: Selector,
    pub content: String,
    pub file_path: PathBuf,
    pub language_name: LanguageName,
}

/// Record of an applied edit, held for undo and retargeting.
///
/// The record stores the complete file content on both sides of the edit
/// rather than a reverse diff: undo is then a byte-exact restore with no
/// patch-application failure modes, and the whole record round-trips through
/// a [`ToolState`] snapshot.
#[derive(Debug, Clone, Fieldwork, Serialize, Deserialize, PartialEq, Eq)]
#[fieldwork(get)]
pub struct AppliedEdit {
    pub operation: EditOperation,
    /// Full file content from before the edit was applied
    pub pre_edit_source: String,
    /// Full file content the edit wrote
    pub post_edit_source: String,
}

impl AppliedEdit {
    /// Whether the file on disk still contains exactly what this edit wrote.
    /// Undo and retarget refuse to act on a stale record — reverting over
    /// someone else's changes would silently discard them.
    pub fn is_current_on_disk(&self) -> std::io::Result<bool> {
        Ok(std::fs::read_to_string(&self.operation.file_path)? == self.post_edit_source)
    }
}

/// The semantic editing tools' shared state, cloned into every call.
///
/// There is no working directory to set: a path is absolute, or relative to
/// the root these tools were created with.
#[derive(Clone)]
pub struct SemanticEditTools {
    root: Arc<Path>,
    language_registry: Arc<LanguageRegistry>,
    commit_fn: Option<CommitFn>,
    /// Shared with clones, so [`set_formatting`](Self::set_formatting) on any of
    /// them reaches calls made through all of them. A call takes one snapshot of
    /// it and uses that throughout.
    formatting: Arc<Mutex<Arc<Formatting>>>,
    state: Arc<Mutex<ToolState>>,
    /// Held for the whole of every call that writes. Calls run concurrently,
    /// and an edit is a read-modify-write of a file: two edits to one file
    /// that each read it first would lose one of them. Edits take
    /// milliseconds, so one lock for all files costs nothing worth a lock
    /// per file.
    write_lock: Arc<Mutex<()>>,
}

impl Debug for SemanticEditTools {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("SemanticEditTools")
            .field("root", &self.root)
            .field("language_registry", &self.language_registry)
            .field("formatting", &self.formatting())
            .field("owns_persistence", &self.owns_persistence())
            .finish_non_exhaustive()
    }
}

impl SemanticEditTools {
    /// Tools that resolve relative paths against `root`, with the default
    /// [`Formatting`].
    pub fn new(root: PathBuf) -> Result<Self> {
        Ok(Self {
            root: root.into(),
            language_registry: Arc::new(LanguageRegistry::new()?),
            commit_fn: None,
            formatting: Arc::default(),
            state: Arc::default(),
            write_lock: Arc::default(),
        })
    }

    /// Persist edits through `commit_fn` instead of writing files.
    #[allow(dead_code, reason = "embedding API, used by library consumers")]
    pub fn with_commit_fn(mut self, commit_fn: CommitFn) -> Self {
        self.commit_fn = Some(commit_fn);
        self
    }

    /// Format edited files as `formatting` says. These settings are this value's
    /// and its later clones': clones made before this call keep theirs.
    #[allow(dead_code, reason = "embedding API, used by library consumers")]
    pub fn with_formatting(mut self, formatting: Formatting) -> Self {
        self.formatting = Arc::new(Mutex::new(Arc::new(formatting)));
        self
    }

    /// Replace the formatting settings, for calls made through this value and
    /// every clone sharing its settings. A call already running finishes with
    /// the settings it started with.
    #[allow(dead_code, reason = "embedding API, used by library consumers")]
    pub fn set_formatting(&self, formatting: Formatting) {
        *lock(&self.formatting) = Arc::new(formatting);
    }

    /// The current formatting settings. A call takes them once and uses that
    /// snapshot throughout, so it sees one consistent set even while
    /// [`set_formatting`](Self::set_formatting) replaces them.
    pub fn formatting(&self) -> Arc<Formatting> {
        lock(&self.formatting).clone()
    }

    pub fn language_registry(&self) -> &LanguageRegistry {
        &self.language_registry
    }

    /// Run a tool body on tokio's blocking pool: parsing and formatting are
    /// synchronous, and must not stall the executor other calls share.
    pub async fn unblock<T: Send + 'static>(
        self,
        body: impl FnOnce(&Self) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        tokio::task::spawn_blocking(move || body(&self)).await?
    }

    /// Take the lock every writing call holds throughout.
    pub fn write_lock(&self) -> MutexGuard<'_, ()> {
        lock(&self.write_lock)
    }

    /// The most recent edit to `path` still held, if any.
    pub fn latest_edit(&self, path: &Path) -> Option<AppliedEdit> {
        self.state()
            .edits
            .iter()
            .rev()
            .find(|edit| edit.operation.file_path == path)
            .cloned()
    }

    /// Hold an applied edit for undo and retarget, dropping the oldest past
    /// [`EDIT_HISTORY_LIMIT`].
    pub fn record_edit(&self, edit: AppliedEdit) {
        let mut state = self.state();
        state.edits.push_back(edit);
        while state.edits.len() > EDIT_HISTORY_LIMIT {
            state.edits.pop_front();
        }
    }

    /// Forget the most recent edit to `path`, once it has been undone or
    /// replaced by a retarget.
    pub fn forget_latest_edit(&self, path: &Path) {
        let mut state = self.state();
        if let Some(index) = state
            .edits
            .iter()
            .rposition(|edit| edit.operation.file_path == path)
        {
            state.edits.remove(index);
        }
    }

    /// Persist edited file content: through the injected commit hook when one
    /// is set (embedders own persistence), otherwise straight to disk.
    pub fn persist_output(&self, path: PathBuf, content: String) -> Result<()> {
        match &self.commit_fn {
            Some(commit) => {
                commit(path, content);
                Ok(())
            }
            None => std::fs::write(path, content).map_err(Into::into),
        }
    }

    /// Whether this instance writes files itself. When an embedder has
    /// injected a commit hook it owns persistence, so the tool cannot verify
    /// disk state against the [`AppliedEdit`] record and skips those checks.
    pub fn owns_persistence(&self) -> bool {
        self.commit_fn.is_none()
    }

    /// A copy of the teaching progress.
    pub fn education(&self) -> EducationState {
        self.state().education.clone()
    }

    /// Update the teaching progress in place.
    pub fn update_education(&self, fun: impl FnOnce(&mut EducationState)) {
        fun(&mut self.state().education);
    }

    /// Everything these tools remember, for an embedder that persists it.
    #[allow(dead_code, reason = "embedding API, used by library consumers")]
    pub fn snapshot(&self) -> ToolState {
        self.state().clone()
    }

    /// Restore what [`snapshot`](Self::snapshot) captured.
    #[allow(dead_code, reason = "embedding API, used by library consumers")]
    pub fn restore(&self, snapshot: ToolState) {
        *self.state() = snapshot;
    }

    /// Resolve a path as a call gave it: `~` expanded, then taken as-is if
    /// absolute and against the root otherwise, and required to exist.
    pub(crate) fn resolve_path(&self, path_str: &str) -> Result<PathBuf> {
        let absolute = self.root.join(&*shellexpand::tilde(path_str));

        // Canonicalize requires the file to exist, so a missing path surfaces here.
        // Give an actionable error instead of a bare OS message: this tool edits
        // *existing* files (it needs a parse tree to target), so creating a new file
        // is a separate concern.
        std::fs::canonicalize(&absolute).map_err(|e| {
            anyhow!(
                "Could not open `{}`: {e}. semantic-edit operates on existing files; \
                 create the file before editing it.",
                absolute.display()
            )
        })
    }

    /// `path` for display: relative to the root when it is under it, since
    /// the absolute path is machine-specific noise in a response.
    pub(crate) fn display_path<'a>(&self, path: &'a Path) -> &'a Path {
        std::fs::canonicalize(&self.root)
            .ok()
            .and_then(|root| path.strip_prefix(root).ok())
            .unwrap_or(path)
    }

    fn state(&self) -> MutexGuard<'_, ToolState> {
        lock(&self.state)
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|e| e.into_inner())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::formatting::Formatter;

    fn tools() -> SemanticEditTools {
        SemanticEditTools::new(std::env::temp_dir()).unwrap()
    }

    fn toml_only() -> Formatting {
        Formatting::off().with(LanguageName::Toml, Formatter::taplo())
    }

    #[test]
    fn new_tools_have_the_default_formatting() {
        assert_eq!(*tools().formatting(), Formatting::default());
    }

    /// An embedder hands one clone to its server and keeps another to change the
    /// settings while it runs.
    #[test]
    fn set_formatting_reaches_every_clone() {
        let tools = tools();
        let served = tools.clone();
        tools.set_formatting(Formatting::off());
        assert_eq!(*served.formatting(), Formatting::off());
        assert_eq!(*served.clone().formatting(), Formatting::off());
    }

    /// A call takes one snapshot, so settings replaced mid-call don't reach it.
    #[test]
    fn a_snapshot_keeps_the_settings_it_was_taken_with() {
        let tools = tools();
        let snapshot = tools.formatting();
        tools.set_formatting(Formatting::off());
        assert_eq!(*snapshot, Formatting::default());
        assert_eq!(*tools.formatting(), Formatting::off());
    }

    #[test]
    fn with_formatting_configures_its_value_and_later_clones_only() {
        let original = tools();
        let configured = original.clone().with_formatting(Formatting::off());
        assert_eq!(*original.formatting(), Formatting::default());
        assert_eq!(*configured.formatting(), Formatting::off());

        configured.clone().set_formatting(toml_only());
        assert_eq!(*configured.formatting(), toml_only());
        assert_eq!(*original.formatting(), Formatting::default());
    }
}
