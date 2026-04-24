//! Runtime registry mapping adapter ids to adapter instances.

use crate::traits::Adapter;
use std::collections::HashMap;
use std::sync::Arc;

/// Collection of registered adapters keyed by their string id.
#[derive(Default)]
pub struct AdapterRegistry {
    by_id: HashMap<String, Arc<dyn Adapter>>,
}

impl AdapterRegistry {
    /// Construct an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an adapter. Overwrites any existing registration.
    pub fn register(&mut self, adapter: Arc<dyn Adapter>) {
        let key = adapter.id().as_str().to_string();
        self.by_id.insert(key, adapter);
    }

    /// Look up by id.
    pub fn get(&self, id: &str) -> Option<Arc<dyn Adapter>> {
        self.by_id.get(id).cloned()
    }

    /// All registered adapters.
    pub fn all(&self) -> Vec<Arc<dyn Adapter>> {
        self.by_id.values().cloned().collect()
    }
}
