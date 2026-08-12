use crate::{
    education::EducationState,
    languages::{LanguageName, LanguageRegistry},
    selector::Selector,
};
use anyhow::{Result, anyhow};
use fieldwork::Fieldwork;
use mcplease::session::SessionStore;
use serde::{Deserialize, Serialize};
use std::{
    fmt::{self, Debug, Formatter},
    path::PathBuf,
    sync::Arc,
};

/// Shared context data that can be used across multiple MCP servers
#[derive(Debug, Clone, Serialize, Deserialize, Default, Eq, PartialEq)]
pub struct SharedContextData {
    /// Current working context path
    context_path: Option<PathBuf>,
}

/// Session data specific to semantic editing operations.
///
/// This struct is the tool's *complete* per-session state, and it is plain
/// serializable data on purpose: an embedder that doesn't persist the session
/// store (e.g. efference's in-memory embedding) can snapshot it via
/// [`SemanticEditTools::session_snapshot`] and restore it via
/// [`SemanticEditTools::restore_session`] so a resumed session picks up
/// identical tool state — undo record and education progress alike.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct SemanticEditSessionData {
    /// The most recently applied edit, retained for undo and retargeting
    #[serde(default)]
    last_edit: Option<AppliedEdit>,
    /// Within-session teaching progress (see [`crate::education`])
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

/// Record of the most recently applied edit, held per session for single-level
/// undo and retargeting.
///
/// The record stores the complete file content on both sides of the edit
/// rather than a reverse diff: undo is then a byte-exact restore with no
/// patch-application failure modes, and the whole record round-trips through
/// the JSON session store, so an undo survives a server restart.
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

/// Semantic editing tools with session support
#[derive(Fieldwork)]
#[fieldwork(get, get_mut)]
pub struct SemanticEditTools {
    /// Private session store for edit-specific state (undo record, education)
    session_store: SessionStore<SemanticEditSessionData>,
    /// Shared context store for cross-server communication
    shared_context_store: SessionStore<SharedContextData>,
    language_registry: Arc<LanguageRegistry>,
    #[field(set, get_mut(option_borrow_inner = false))]
    commit_fn: Option<Box<dyn Fn(PathBuf, String) + 'static>>,
    #[field(set, with)]
    default_session_id: &'static str,
}

impl Debug for SemanticEditTools {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.debug_struct("SemanticEditTools")
            .field("session_store", &self.session_store)
            .field("shared_context_store", &self.shared_context_store)
            .field("language_registry", &self.language_registry)
            .field("default_session_id", &self.default_session_id)
            .finish()
    }
}

impl SemanticEditTools {
    /// Create a new SemanticEditTools instance
    pub fn new(storage_path: Option<&str>) -> Result<Self> {
        // Private session store for edit-specific state
        let private_path = storage_path.map(|s| PathBuf::from(&*shellexpand::tilde(s)));
        let session_store = SessionStore::new(private_path)?;

        // Shared context store for cross-server communication
        let mut shared_path = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        shared_path.push(".ai-tools");
        shared_path.push("sessions");
        shared_path.push("shared-context.json");
        let shared_context_store = SessionStore::new(Some(shared_path))?;

        let language_registry = Arc::new(LanguageRegistry::new()?);

        Ok(Self {
            session_store,
            shared_context_store,
            language_registry,
            commit_fn: None,
            default_session_id: "default",
        })
    }

    /// Construct for in-process embedding (e.g. the efference harness): both
    /// session stores **in memory** (no disk, no cross-process sharing), the
    /// language registry built, and the working directory pre-seeded so relative
    /// paths resolve. Unlike [`new`](Self::new) it never touches the shared
    /// `~/.ai-tools` store.
    #[allow(
        dead_code,
        reason = "used by library consumers (embedding), not the bin"
    )]
    pub fn embedded(working_directory: PathBuf) -> Result<Self> {
        let mut tools = Self {
            session_store: SessionStore::new(None)?,
            shared_context_store: SessionStore::new(None)?,
            language_registry: Arc::new(LanguageRegistry::new()?),
            commit_fn: None,
            default_session_id: "default",
        };
        tools.set_working_directory(working_directory, None)?;
        Ok(tools)
    }

    /// Get context for a session
    pub fn get_context(&mut self, session_id: Option<&str>) -> Result<Option<PathBuf>> {
        let session_id = session_id.unwrap_or_else(|| self.default_session_id());
        let shared_data = self.shared_context_store.get_or_create(session_id)?;
        Ok(shared_data.context_path.clone())
    }

    /// Record the most recently applied edit (or clear it with `None`),
    /// replacing any previous record — undo is single-level by design.
    pub fn set_last_edit(
        &mut self,
        session_id: Option<&str>,
        last_edit: Option<AppliedEdit>,
    ) -> Result<()> {
        let session_id = session_id.unwrap_or_else(|| self.default_session_id());
        self.session_store.update(session_id, |data| {
            data.last_edit = last_edit;
        })
    }

    /// The most recently applied edit, if any
    pub fn last_edit(&mut self, session_id: Option<&str>) -> Result<Option<&AppliedEdit>> {
        let session_id = session_id.unwrap_or_else(|| self.default_session_id());
        let session_data = self.session_store.get_or_create(session_id)?;
        Ok(session_data.last_edit.as_ref())
    }

    /// Persist edited file content: through the injected commit hook when one
    /// is set (embedders own persistence), otherwise straight to disk.
    pub fn persist_output(&mut self, path: PathBuf, content: String) -> Result<()> {
        // Borrow (don't `take`) the commit hook so it survives across multiple
        // persists — embedders set it once and persist many times.
        if let Some(commit) = self.commit_fn_mut() {
            commit(path, content);
            Ok(())
        } else {
            std::fs::write(path, content).map_err(Into::into)
        }
    }

    /// Whether this instance writes files itself. When an embedder has
    /// injected a commit hook it owns persistence, so the tool cannot verify
    /// disk state against the [`AppliedEdit`] record and skips those checks.
    pub fn owns_persistence(&self) -> bool {
        self.commit_fn.is_none()
    }

    /// A copy of the session's education state.
    pub fn education(&mut self, session_id: Option<&str>) -> Result<EducationState> {
        let session_id = session_id.unwrap_or_else(|| self.default_session_id());
        Ok(self
            .session_store
            .get_or_create(session_id)?
            .education
            .clone())
    }

    /// Update the session's education state in place.
    pub fn update_education<F>(&mut self, session_id: Option<&str>, fun: F) -> Result<()>
    where
        F: FnOnce(&mut EducationState),
    {
        let session_id = session_id.unwrap_or_else(|| self.default_session_id());
        self.session_store
            .update(session_id, |data| fun(&mut data.education))
    }

    /// Snapshot the complete per-session tool state, for embedders that manage
    /// persistence themselves (e.g. serializing into a session log so
    /// resumption restores identical tool state).
    #[allow(dead_code, reason = "embedding API, used by library consumers")]
    pub fn session_snapshot(
        &mut self,
        session_id: Option<&str>,
    ) -> Result<SemanticEditSessionData> {
        let session_id = session_id.unwrap_or_else(|| self.default_session_id());
        Ok(self.session_store.get_or_create(session_id)?.clone())
    }

    /// Restore per-session tool state captured by [`session_snapshot`](Self::session_snapshot).
    #[allow(dead_code, reason = "embedding API, used by library consumers")]
    pub fn restore_session(
        &mut self,
        session_id: Option<&str>,
        snapshot: SemanticEditSessionData,
    ) -> Result<()> {
        let session_id = session_id.unwrap_or_else(|| self.default_session_id());
        self.session_store
            .update(session_id, |data| *data = snapshot)
    }

    /// Set context path for a session
    pub fn set_working_directory(&mut self, path: PathBuf, session_id: Option<&str>) -> Result<()> {
        let session_id = session_id.unwrap_or_else(|| self.default_session_id());

        // Setting the working directory is the closest signal available for "a
        // new session began", so within-session teaching restarts from zero
        // here — every session starts untaught by design.
        self.session_store.update(session_id, |data| {
            data.education = EducationState::default();
        })?;

        self.shared_context_store_mut().update(session_id, |data| {
            data.context_path = Some(path);
        })
    }

    #[allow(dead_code, reason = "used in tests")]
    pub fn with_working_directory(
        mut self,
        path: PathBuf,
        session_id: Option<&str>,
    ) -> Result<Self> {
        self.set_working_directory(path, session_id)?;
        Ok(self)
    }

    /// Resolve a path relative to session context if needed
    pub(crate) fn resolve_path(
        &mut self,
        path_str: &str,
        session_id: Option<&str>,
    ) -> Result<PathBuf> {
        let path = PathBuf::from(&*shellexpand::tilde(path_str));

        let absolute = if path.is_absolute() {
            path
        } else {
            let session_id = session_id.unwrap_or_else(|| self.default_session_id());
            match self.get_context(Some(session_id))? {
                Some(context) => context.join(path_str),
                None => {
                    return Err(anyhow!(
                        "No context found for `{session_id}`. Use set_working_directory first or provide an absolute path.",
                    ));
                }
            }
        };

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
}
