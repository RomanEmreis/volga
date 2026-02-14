//! Types and utils for the OpenAPI registry.

use std::{collections::BTreeMap, sync::{Arc, Mutex}};
use http::Method;

use super::{
    doc::{OpenApiDocument, OpenApiComponents, OpenApiInfo},
    config::{OpenApiConfig, OpenApiSpec},
    route::OpenApiRouteConfig,
    op::OpenApiOperation
};

const DEFAULT_OPENAPI_VERSION: &str = "3.0.0";

/// OpenAPI runtime registry.
#[derive(Clone, Debug)]
pub struct OpenApiRegistry {
    inner: Arc<Mutex<BTreeMap<String, OpenApiDocument>>>,
    specs: Vec<OpenApiSpec>,
    ui_path: String,
    ui_enabled: bool,
}

impl OpenApiRegistry {
    /// Creates a new [`OpenApiRegistry`]
    pub fn new(config: OpenApiConfig) -> Self {
        let base_doc = |title: String, version: String, description: Option<String>| OpenApiDocument {
            openapi: DEFAULT_OPENAPI_VERSION.to_string(),
            info: OpenApiInfo { title, version, description },
            paths: BTreeMap::new(),
            components: OpenApiComponents { schemas: BTreeMap::new() },
        };

        let mut docs = BTreeMap::new();
        for s in &config.specs {
            docs.insert(
                s.name.clone(),
                base_doc(
                    config.title.clone(),
                    config.version.clone(),
                    config.description.clone()
                ),
            );
        }

        Self {
            inner: Arc::new(Mutex::new(docs)),
            specs: config.specs,
            ui_path: config.ui_path,
            ui_enabled: config.ui_enabled,
        }
    }

    /// Registers a route in OpenAPI registry.
    pub fn register_route(
        &self,
        method: &Method,
        path: &str,
        cfg: &OpenApiRouteConfig
    ) {
        if self.is_excluded_path(path) {
            return;
        }

        let mut docs = self.lock();
        let method = method.as_str().to_ascii_lowercase();
        let targets = self.target_doc_names(cfg);

        for doc_name in targets {
            if let Some(doc) = docs.get_mut(doc_name) {
                let entry = doc.paths
                    .entry(path.to_string())
                    .or_default();

                entry
                    .entry(method.clone())
                    .or_insert_with(|| OpenApiOperation::for_method(method.clone(), path));
            }
        }
    }

    /// Rebinds route to another spec.
    pub fn rebind_route(
        &self,
        method: &Method,
        path: &str,
        cfg: &OpenApiRouteConfig,
    ) {
        let method_lc = method.as_str().to_ascii_lowercase();
        let targets = self.target_doc_names(cfg);

        let mut docs = self.lock();

        let mut op_opt: Option<OpenApiOperation> = None;
        for doc in docs.values_mut() {
            if let Some(methods) = doc.paths.get_mut(path)
                && let Some(op) = methods.remove(&method_lc) {
                op_opt = Some(op);
                if methods.is_empty() { doc.paths.remove(path); }
                break;
            }
        }

        let mut op = op_opt
            .unwrap_or_else(|| OpenApiOperation::for_method(method_lc.clone(), path));

        for name in targets {
            let Some(doc) = docs.get_mut(name) else { continue; };

            cfg.apply_to_operation(&mut op, &mut doc.components.schemas);

            doc.paths
                .entry(path.to_string())
                .or_default()
                .insert(method_lc.clone(), op.clone());
        }
    }

    /// Applies route configuration
    pub fn apply_route_config(
        &self,
        method: &Method,
        path: &str,
        cfg: &OpenApiRouteConfig,
    ) {
        if self.is_excluded_path(path) {
            return;
        }

        let mut docs = self.lock();
        let method_lc = method.as_str().to_ascii_lowercase();
        let targets = self.target_doc_names(cfg);

        for doc_name in targets {
            let Some(doc) = docs.get_mut(doc_name) else { continue; };

            let OpenApiDocument { paths, components, .. } = doc;

            let entry = paths.entry(path.to_string()).or_default();
            let op = entry.entry(method_lc.clone())
                .or_insert_with(|| OpenApiOperation::for_method(method_lc.clone(), path));

            cfg.apply_to_operation(op, &mut components.schemas);
        }
    }

    /// Returns OpenAPI document by spec name.
    pub fn document_by_name(&self, name: &str) -> Option<OpenApiDocument> {
        self.lock().get(name).cloned()
    }

    /// Returns a list of defined specs
    pub fn specs(&self) -> &[OpenApiSpec] {
        &self.specs
    }

    fn is_excluded_path(&self, path: &str) -> bool {
        if self.ui_enabled && path == self.ui_path { return true; }
        self.specs.iter().any(|s| s.spec_path == path)
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, BTreeMap<String, OpenApiDocument>> {
        self.inner.lock().expect("openapi registry lock poisoned")
    }

    fn target_doc_names<'a>(&'a self, cfg: &'a OpenApiRouteConfig) -> Vec<&'a str> {
        if let Some(docs) = cfg.docs() {
            docs.iter().map(|s| s.as_str()).collect()
        } else {
            self.specs
                .first()
                .map(|s| vec![s.name.as_str()])
                .unwrap_or_default()
        }
    }
}
