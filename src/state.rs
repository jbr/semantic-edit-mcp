use std::num::NonZeroUsize;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use anyhow::{anyhow, Result};
use fieldwork::Fieldwork;
use lru::LruCache;
use serde::{Deserialize, Serialize};

use crate::editor::EditPosition;
use crate::languages::{LanguageName, LanguageRegistry};
use crate::selector::Selector;
use crate::session::SessionStore;

// Explanation for the presence of session_id that is currently unused: The intent was initially to
// have a conversation-unique identifier of some sort in order to isolate state between
// conversations. However, MCP provides no mechanism to distinguish between conversations, so I
// tried adding a session_id that was provided to every tool call in order to isolate state. This
// presents a usability concern, so I've decided to just be extra careful about switching contexts
// until we have a better solution. I still hope to iterate towards isolated sessions, so the code
// is still written to support that.

/// Session data specific to semantic editing operations
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct SemanticEditSessionData {
    /// Current working context path
    pub context_path: Option<PathBuf>,
    /// Currently staged operation
    pub staged_operation: Option<StagedOperation>,
}

/// Represents a staged operation that can be previewed and committed
#[derive(Debug, Clone, Fieldwork, Serialize, Deserialize)]
#[fieldwork(get, set, get_mut, with)]
pub struct StagedOperation {
    pub selector: Selector,
    pub content: String,
    pub file_path: PathBuf,
    pub language_name: LanguageName,
    pub edit_position: Option<EditPosition>,
}

impl StagedOperation {
    pub fn retarget(&mut self, selector: Selector) {
        self.selector = selector;
    }
}

/// Semantic editing tools with session support
#[derive(fieldwork::Fieldwork)]
#[fieldwork(get)]
pub struct SemanticEditTools {
    #[fieldwork(get_mut)]
    session_store: SessionStore<SemanticEditSessionData>,
    language_registry: Arc<LanguageRegistry>,
    file_cache: Arc<Mutex<LruCache<String, String>>>,
    #[fieldwork(set, get_mut, option = false)]
    commit_fn: Option<Box<(dyn Fn(PathBuf, String) + 'static)>>,
    #[fieldwork(set, with)]
    default_session_id: &'static str,
}

impl std::fmt::Debug for SemanticEditTools {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SemanticEditTools")
            .field("session_store", &self.session_store)
            .field("language_registry", &self.language_registry)
            .field("file_cache", &self.file_cache)
            .field("default_session_id", &self.default_session_id)
            .finish()
    }
}

impl SemanticEditTools {
    /// Create a new SemanticEditTools instance
    pub fn new(storage_path: Option<&str>) -> Result<Self> {
        let storage_path = storage_path.map(|s| PathBuf::from(&*shellexpand::tilde(s)));
        let session_store = SessionStore::new(storage_path)?;
        let language_registry = Arc::new(LanguageRegistry::new()?);
        let file_cache = Arc::new(Mutex::new(LruCache::new(NonZeroUsize::new(50).unwrap())));

        Ok(Self {
            session_store,
            language_registry,
            file_cache,
            commit_fn: None,
            default_session_id: "default",
        })
    }

    /// Get context for a session
    pub fn get_context(&self, session_id: Option<&str>) -> Result<Option<PathBuf>> {
        let session_id = session_id.unwrap_or_else(|| self.default_session_id());
        let session_data = self.session_store.get_or_create(session_id)?;
        Ok(session_data.context_path)
    }

    /// Stage a new operation, replacing any existing staged operation
    pub fn stage_operation(
        &self,
        session_id: Option<&str>,
        staged_operation: Option<StagedOperation>,
    ) -> Result<()> {
        let session_id = session_id.unwrap_or_else(|| self.default_session_id());
        self.session_store.update(session_id, |data| {
            data.staged_operation = staged_operation;
        })
    }

    /// Get the currently staged operation, if any
    pub fn get_staged_operation(
        &self,
        session_id: Option<&str>,
    ) -> Result<Option<StagedOperation>> {
        let session_id = session_id.unwrap_or_else(|| self.default_session_id());
        let session_data = self.session_store.get_or_create(session_id)?;
        Ok(session_data.staged_operation)
    }

    /// Take the staged operation, removing it from storage
    pub fn take_staged_operation(
        &self,
        session_id: Option<&str>,
    ) -> Result<Option<StagedOperation>> {
        let mut staged_op = None;
        let session_id = session_id.unwrap_or_else(|| self.default_session_id());
        self.session_store.update(session_id, |data| {
            staged_op = data.staged_operation.take();
        })?;
        Ok(staged_op)
    }

    /// Modify the staged operation in place
    pub fn modify_staged_operation<F>(
        &self,
        session_id: Option<&str>,
        fun: F,
    ) -> Result<Option<StagedOperation>>
    where
        F: FnOnce(&mut StagedOperation),
    {
        let session_id = session_id.unwrap_or_else(|| self.default_session_id());
        self.session_store.update(session_id, |data| {
            if let Some(ref mut op) = data.staged_operation {
                fun(op);
            }
        })?;
        self.get_staged_operation(Some(session_id))
    }

    /// Set context path for a session
    pub fn set_context(&self, session_id: Option<&str>, path: PathBuf) -> Result<()> {
        let session_id = session_id.unwrap_or_else(|| self.default_session_id());

        self.session_store.update(session_id, |data| {
            data.context_path = Some(path);
        })
    }

    /// Resolve a path relative to session context if needed
    pub(crate) fn resolve_path(&self, path_str: &str, session_id: Option<&str>) -> Result<PathBuf> {
        let path = PathBuf::from(&*shellexpand::tilde(path_str));

        if path.is_absolute() {
            return Ok(std::fs::canonicalize(path)?);
        }

        let session_id = session_id.unwrap_or_else(|| self.default_session_id());

        match self.get_context(Some(session_id))? {
            Some(context) => {
                Ok(std::fs::canonicalize(context.join(path_str))?)
            },
            None => Err(anyhow!(
                "No context found for `{session_id}`. Use set_context first or provide an absolute path.",
            )),
        }
    }
}
