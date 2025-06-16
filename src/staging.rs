use anyhow::Result;

use crate::{languages::LanguageRegistry, operations::EditOperation, tools::ExecutionResult};
use std::sync::{Arc, Mutex};

/// Represents a staged operation that can be previewed and committed
#[derive(Debug, Clone, fieldwork::Fieldwork)]
#[fieldwork(get, set, get_mut, with)]
pub struct StagedOperation {
    pub(crate) operation: EditOperation,
    pub(crate) file_path: String,
    pub(crate) language_name: &'static str,
}

impl StagedOperation {
    pub fn commit(self, language_registry: &LanguageRegistry) -> Result<ExecutionResult> {
        let StagedOperation {
            operation,
            file_path,
            language_name,
        } = self;
        let language = language_registry
            .get_language(language_name)
            .ok_or_else(|| anyhow::anyhow!("language not recognized"))?;

        // Apply the operation for real (preview_only=false)
        operation.apply(language, &file_path, false)
    }
}

/// Thread-safe staging storage for the server
#[derive(Debug, Clone)]
pub struct StagingStore {
    staged_operation: Arc<Mutex<Option<StagedOperation>>>,
}

impl StagingStore {
    pub fn new() -> Self {
        Self {
            staged_operation: Arc::new(Mutex::new(None)),
        }
    }

    /// Stage a new operation, replacing any existing staged operation
    pub fn stage(&self, staged_operation: StagedOperation) {
        let mut guard = self.staged_operation.lock().unwrap();
        *guard = Some(staged_operation);
    }

    /// Get the currently staged operation, if any
    pub fn get_staged_operation(&self) -> Option<StagedOperation> {
        let guard = self.staged_operation.lock().unwrap();
        guard.clone()
    }

    /// Take the staged operation, removing it from storage
    pub fn take_staged_operation(&self) -> Option<StagedOperation> {
        let mut guard = self.staged_operation.lock().unwrap();
        guard.take()
    }

    /// Check if there's a staged operation
    pub fn has_staged_operation(&self) -> bool {
        let guard = self.staged_operation.lock().unwrap();
        guard.is_some()
    }
}

impl Default for StagingStore {
    fn default() -> Self {
        Self::new()
    }
}
