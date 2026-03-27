//! Type-erased per-section storage for the config system.

use arc_swap::ArcSwap;
use serde::de::DeserializeOwned;
use serde_json::Value;
use std::{
    any::{Any, TypeId},
    collections::HashMap,
    path::PathBuf,
    sync::Arc,
    time::Duration,
};

/// Controls how a missing section is treated at startup and at reload time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SectionKind {
    /// Section must be present at startup. If it disappears during reload, keep old value.
    Required,
    /// Section may be absent. If it disappears during reload, store `None`.
    Optional,
}

/// Type-erased reload interface for a single config section.
trait ErasedSection: Send + Sync {
    fn reload(&self, full_value: &Value);
}

/// Concrete per-section storage for type `T`.
///
/// Stores `Option<Arc<T>>` inside the `ArcSwap` so that `get<T>()` can return a
/// cloneable `Arc<T>` directly without extra allocation.
struct SectionStore<T: Send + Sync + 'static> {
    key: String,
    kind: SectionKind,
    swap: Arc<ArcSwap<Option<Arc<T>>>>,
}

impl<T: DeserializeOwned + Send + Sync + 'static> ErasedSection for SectionStore<T> {
    fn reload(&self, full_value: &Value) {
        match full_value
            .get(&self.key)
            .map(|v| serde_json::from_value::<T>(v.clone()))
        {
            Some(Ok(new_val)) => {
                self.swap.store(Arc::new(Some(Arc::new(new_val))));
            }
            Some(Err(_err)) => {
                // Keep previous value.
                #[cfg(feature = "tracing")]
                tracing::error!(
                    "config reload: failed to parse section '{}': {_err:#}",
                    self.key
                );
            }
            None => {
                if self.kind == SectionKind::Optional {
                    self.swap.store(Arc::new(None));
                } else {
                    // Required section disappeared — keep old value.
                    #[cfg(feature = "tracing")]
                    tracing::error!(
                        "config reload: required section '{}' is missing; keeping previous value",
                        self.key
                    );
                }
            }
        }
    }
}

/// Holds all pre-deserialized user config sections.
///
/// Stored as `Arc<ConfigStore>` in `AppEnv` and cloned into request extensions.
/// `Config<T>` reads from it via `get::<T>()` — one atomic load + `Arc::clone` per request.
pub struct ConfigStore {
    /// Keyed by TypeId — used by `reload_sections()` to update all sections.
    sections: HashMap<TypeId, Box<dyn ErasedSection>>,
    /// Parallel map for type-safe downcast in `get<T>()` via `Any`.
    ///
    /// **Invariant:** `sections` and `values` are parallel structures. Every `register` call
    /// must insert into both maps with the same `TypeId` key. Never insert into one without
    /// the other.
    values: HashMap<TypeId, Arc<dyn Any + Send + Sync>>,
    /// Hot-reload scheduling info: `(interval, file_path)`.
    ///
    /// `None` means no hot-reload is configured. Set by [`ConfigBuilder::build_from_value`].
    pub(crate) reload: Option<(Duration, PathBuf)>,
}

impl std::fmt::Debug for ConfigStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ConfigStore")
            .field("section_count", &self.sections.len())
            .finish()
    }
}

impl Default for ConfigStore {
    fn default() -> Self {
        Self::new()
    }
}

impl ConfigStore {
    /// Creates an empty store.
    pub fn new() -> Self {
        Self {
            sections: HashMap::new(),
            values: HashMap::new(),
            reload: None,
        }
    }

    /// Parses and registers a section for type `T`.
    ///
    /// Returns `Err` if the section is `Required` but absent or malformed,
    /// or if `T` has already been registered (duplicate binding).
    /// For `Optional`, a missing section is stored as `None` without error.
    pub fn register<T>(
        &mut self,
        key: &str,
        kind: SectionKind,
        full_value: &Value,
    ) -> Result<(), String>
    where
        T: DeserializeOwned + Send + Sync + 'static,
    {
        if self.sections.contains_key(&TypeId::of::<T>()) {
            return Err(format!(
                "config: type '{}' is already registered",
                std::any::type_name::<T>()
            ));
        }

        let initial: Option<Arc<T>> = match full_value
            .get(key)
            .map(|v| serde_json::from_value::<T>(v.clone()))
        {
            Some(Ok(v)) => Some(Arc::new(v)),
            Some(Err(e)) => return Err(format!("config: section '{key}' failed to parse: {e}")),
            None if kind == SectionKind::Required => {
                return Err(format!("config: required section '{key}' is missing"));
            }
            None => None,
        };

        let swap: Arc<ArcSwap<Option<Arc<T>>>> = Arc::new(ArcSwap::from_pointee(initial));
        let entry = SectionStore::<T> {
            key: key.to_owned(),
            kind,
            swap: Arc::clone(&swap),
        };
        self.sections.insert(TypeId::of::<T>(), Box::new(entry));
        self.values
            .insert(TypeId::of::<T>(), swap as Arc<dyn Any + Send + Sync>);
        Ok(())
    }

    /// Returns the current `Arc<T>` for the section, or `None` if optional and absent.
    pub fn get<T: Send + Sync + 'static>(&self) -> Option<Arc<T>> {
        let any = self.values.get(&TypeId::of::<T>())?;
        let swap = any.downcast_ref::<ArcSwap<Option<Arc<T>>>>()?;
        let guard = swap.load();
        guard.as_ref().clone()
    }

    /// Reloads all sections from a new full config value.
    ///
    /// Per-section errors are logged (if `tracing` feature is on) and the previous
    /// value is retained. This never panics.
    pub fn reload_sections(&self, full_value: &Value) {
        for section in self.sections.values() {
            section.reload(full_value);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq)]
    struct MyConfig {
        value: u32,
    }

    #[test]
    fn required_section_present_returns_arc() {
        let mut store = ConfigStore::new();
        let json = serde_json::json!({ "my": { "value": 42 } });
        store
            .register::<MyConfig>("my", SectionKind::Required, &json)
            .unwrap();

        let arc = store.get::<MyConfig>().unwrap();
        assert_eq!(arc.value, 42);
    }

    #[test]
    fn optional_section_absent_returns_none() {
        let mut store = ConfigStore::new();
        let json = serde_json::json!({});
        store
            .register::<MyConfig>("my", SectionKind::Optional, &json)
            .unwrap();

        assert!(store.get::<MyConfig>().is_none());
    }

    #[test]
    fn required_section_absent_returns_err() {
        let mut store = ConfigStore::new();
        let json = serde_json::json!({});
        let result = store.register::<MyConfig>("my", SectionKind::Required, &json);
        assert!(result.is_err());
    }

    #[test]
    fn reload_updates_value() {
        let mut store = ConfigStore::new();
        let json = serde_json::json!({ "my": { "value": 1 } });
        store
            .register::<MyConfig>("my", SectionKind::Required, &json)
            .unwrap();

        let new_json = serde_json::json!({ "my": { "value": 99 } });
        store.reload_sections(&new_json);

        let arc = store.get::<MyConfig>().unwrap();
        assert_eq!(arc.value, 99);
    }

    #[test]
    fn reload_keeps_old_on_parse_error() {
        let mut store = ConfigStore::new();
        let json = serde_json::json!({ "my": { "value": 7 } });
        store
            .register::<MyConfig>("my", SectionKind::Required, &json)
            .unwrap();

        let bad_json = serde_json::json!({ "my": { "value": "not_a_number" } });
        store.reload_sections(&bad_json);

        let arc = store.get::<MyConfig>().unwrap();
        assert_eq!(arc.value, 7);
    }

    #[test]
    fn reload_optional_section_disappears() {
        let mut store = ConfigStore::new();
        let json = serde_json::json!({ "my": { "value": 5 } });
        store
            .register::<MyConfig>("my", SectionKind::Optional, &json)
            .unwrap();

        let new_json = serde_json::json!({});
        store.reload_sections(&new_json);

        assert!(store.get::<MyConfig>().is_none());
    }

    #[test]
    fn duplicate_registration_returns_err() {
        let mut store = ConfigStore::new();
        let json = serde_json::json!({ "my": { "value": 1 } });
        store
            .register::<MyConfig>("my", SectionKind::Required, &json)
            .unwrap();

        let result = store.register::<MyConfig>("my", SectionKind::Optional, &json);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("already registered"));
    }

    #[test]
    fn reload_optional_section_appears() {
        let mut store = ConfigStore::new();
        let json = serde_json::json!({});
        store
            .register::<MyConfig>("my", SectionKind::Optional, &json)
            .unwrap();
        assert!(store.get::<MyConfig>().is_none());

        let new_json = serde_json::json!({ "my": { "value": 3 } });
        store.reload_sections(&new_json);

        let arc = store.get::<MyConfig>().unwrap();
        assert_eq!(arc.value, 3);
    }

    #[test]
    fn reload_required_section_disappears_keeps_value() {
        let mut store = ConfigStore::new();
        let json = serde_json::json!({ "my": { "value": 10 } });
        store
            .register::<MyConfig>("my", SectionKind::Required, &json)
            .unwrap();

        // Reload with no section — required section disappears; old value kept.
        store.reload_sections(&serde_json::json!({}));

        let arc = store.get::<MyConfig>().unwrap();
        assert_eq!(arc.value, 10);
    }

    #[test]
    fn malformed_section_returns_err() {
        let mut store = ConfigStore::new();
        // "value" must be u32 but is a string — register must return Err.
        let json = serde_json::json!({ "my": { "value": "bad" } });
        let result = store.register::<MyConfig>("my", SectionKind::Required, &json);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("failed to parse"));
    }

    #[test]
    fn debug_impl_is_non_empty() {
        let store = ConfigStore::new();
        assert!(!format!("{store:?}").is_empty());
    }

    #[test]
    fn default_impl_creates_empty_store() {
        let store = ConfigStore::default();
        assert!(store.get::<MyConfig>().is_none());
    }
}
