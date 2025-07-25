use crate::{
    editor::EditPosition,
    languages::{LanguageName, LanguageRegistry},
    selector::Selector,
};
use anyhow::{anyhow, Result};
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

/// Session data specific to semantic editing operations
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct SemanticEditSessionData {
    /// Currently staged operation
    staged_operation: Option<StagedOperation>,
}

/// Represents a staged operation that can be previewed and committed
#[derive(Debug, Clone, Fieldwork, Serialize, Deserialize, PartialEq, Eq)]
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
#[derive(Fieldwork)]
#[fieldwork(get, get_mut)]
pub struct SemanticEditTools {
    /// Private session store for edit-specific state (staged operations, etc.)
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

    /// Get context for a session
    pub fn get_context(&mut self, session_id: Option<&str>) -> Result<Option<PathBuf>> {
        let session_id = session_id.unwrap_or_else(|| self.default_session_id());
        let shared_data = self.shared_context_store.get_or_create(session_id)?;
        Ok(shared_data.context_path.clone())
    }

    /// Stage a new operation, replacing any existing staged operation
    pub fn preview_edit(
        &mut self,
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
        &mut self,
        session_id: Option<&str>,
    ) -> Result<Option<&StagedOperation>> {
        let session_id = session_id.unwrap_or_else(|| self.default_session_id());
        let session_data = self.session_store.get_or_create(session_id)?;
        Ok(session_data.staged_operation.as_ref())
    }

    /// Take the staged operation, removing it from storage
    pub fn take_staged_operation(
        &mut self,
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
        &mut self,
        session_id: Option<&str>,
        fun: F,
    ) -> Result<Option<&StagedOperation>>
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
    pub fn set_working_directory(&mut self, path: PathBuf, session_id: Option<&str>) -> Result<()> {
        let session_id = session_id.unwrap_or_else(|| self.default_session_id());

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

        if path.is_absolute() {
            return Ok(std::fs::canonicalize(path)?);
        }

        let session_id = session_id.unwrap_or_else(|| self.default_session_id());

        match self.get_context(Some(session_id))? {
            Some(context) => Ok(std::fs::canonicalize(context.join(path_str))?),
            None => Err(anyhow!(
                "No context found for `{session_id}`. Use set_context first or provide an absolute path.",
            )),
        }
    }
}
