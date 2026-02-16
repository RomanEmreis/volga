//! Types and utils for the OpenAPI registry.

use std::{collections::BTreeMap, sync::{Arc, Mutex}};
use http::Method;

use super::{
    doc::{OpenApiDocument, OpenApiComponents, OpenApiInfo},
    config::{OpenApiConfig, OpenApiSpec},
    route::OpenApiRouteConfig,
    op::OpenApiOperation,
    param::normalize_openapi_path
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
        let base_doc =
            |title: String, version: String, description: Option<String>| OpenApiDocument {
                openapi: DEFAULT_OPENAPI_VERSION.to_string(),
                info: OpenApiInfo {
                    title,
                    version,
                    description,
                },
                paths: BTreeMap::new(),
                components: OpenApiComponents {
                    schemas: BTreeMap::new(),
                },
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

        let (spec_path, path_params) = normalize_openapi_path(path);

        let mut docs = self.lock();
        let method = method.as_str().to_ascii_lowercase();
        let targets = self.target_doc_names(cfg);

        for doc_name in targets {
            if let Some(doc) = docs.get_mut(doc_name) {
                let entry = doc.paths
                    .entry(spec_path.clone())
                    .or_default();

                let op = entry
                    .entry(method.clone())
                    .or_insert_with(OpenApiOperation::default);

                if op.parameters.is_none() && !path_params.is_empty() {
                    op.parameters = Some(path_params.clone());
                }
            }
        }
    }

    /// Rebinds route to another spec.
    pub fn rebind_route(&self, method: &Method, path: &str, cfg: &OpenApiRouteConfig) {
        if self.is_excluded_path(path) {
            return;
        }

        let (spec_path, path_params) = normalize_openapi_path(path);
        
        let method_lc = method.as_str().to_ascii_lowercase();
        let targets = self.target_doc_names(cfg);

        let mut docs = self.lock();

        let mut op_opt: Option<OpenApiOperation> = None;
        for doc in docs.values_mut() {
            if let Some(methods) = doc.paths.get_mut(&spec_path)
                && let Some(op) = methods.remove(&method_lc)
            {
                op_opt = Some(op);
                if methods.is_empty() { 
                    doc.paths.remove(&spec_path);
                }
            }
        }

        let mut op = op_opt.unwrap_or_default();

        if op.parameters.is_none() && !path_params.is_empty() {
            op.parameters = Some(path_params.clone());
        }

        for name in targets {
            let Some(doc) = docs.get_mut(name) else { 
                continue;
            };

            cfg.apply_to_operation(&mut op, &mut doc.components.schemas);

            doc.paths
                .entry(spec_path.clone())
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

        let (spec_path, path_params) = normalize_openapi_path(path);

        let mut docs = self.lock();
        let method_lc = method.as_str().to_ascii_lowercase();
        let targets = self.target_doc_names(cfg);

        for doc_name in targets {
            let Some(doc) = docs.get_mut(doc_name) else { 
                continue;
            };

            let OpenApiDocument { paths, components, .. } = doc;

            let entry = paths.entry(spec_path.clone()).or_default();
            let op = entry.entry(method_lc.clone())
                .or_insert_with(OpenApiOperation::default);

            if op.parameters.is_none() && !path_params.is_empty() {
                op.parameters = Some(path_params.clone());
            }

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

#[cfg(test)]
mod tests {
    use super::*;

    fn config_with_specs() -> OpenApiConfig {
        OpenApiConfig::new()
            .with_specs([OpenApiSpec::new("v1"), OpenApiSpec::new("admin")])
            .with_ui()
            .with_ui_path("/openapi")
    }

    #[test]
    fn register_route_skips_ui_and_spec_paths() {
        let registry = OpenApiRegistry::new(config_with_specs());

        registry.register_route(&Method::GET, "/openapi", &OpenApiRouteConfig::default());
        registry.register_route(
            &Method::GET,
            "v1/openapi.json",
            &OpenApiRouteConfig::default(),
        );
        registry.register_route(&Method::GET, "/users", &OpenApiRouteConfig::default());

        let v1_doc = registry.document_by_name("v1").expect("v1 document");
        assert!(v1_doc.paths.contains_key("/users"));
        assert!(!v1_doc.paths.contains_key("/openapi"));
        assert!(!v1_doc.paths.contains_key("/sv1/openapi.json"));
    }

    #[test]
    fn rebind_route_moves_operation_to_target_doc() {
        let registry = OpenApiRegistry::new(config_with_specs());

        registry.register_route(&Method::POST, "/pets", &OpenApiRouteConfig::default());
        registry.rebind_route(
            &Method::POST,
            "/pets",
            &OpenApiRouteConfig::default().with_doc("admin"),
        );

        let v1_doc = registry.document_by_name("v1").expect("v1 document");
        let admin_doc = registry.document_by_name("admin").expect("admin document");

        assert!(!v1_doc.paths.contains_key("/pets"));
        assert!(admin_doc.paths.contains_key("/pets"));
        assert!(admin_doc.paths["/pets"].contains_key("post"));
    }

    #[test]
    fn rebind_route_removes_stale_operation_from_all_docs() {
        let registry = OpenApiRegistry::new(config_with_specs());

        let shared = OpenApiRouteConfig::default().with_docs(["v1", "admin"]);
        registry.register_route(&Method::GET, "/shared", &shared);

        let target = OpenApiRouteConfig::default().with_doc("v1");
        registry.rebind_route(&Method::GET, "/shared", &target);

        let v1_doc = registry.document_by_name("v1").expect("v1 document");
        let admin_doc = registry.document_by_name("admin").expect("admin document");

        assert!(v1_doc.paths["/shared"].contains_key("get"));
        assert!(!admin_doc.paths.contains_key("/shared"));
    }

    #[test]
    fn rebind_route_skips_excluded_paths() {
        let registry = OpenApiRegistry::new(config_with_specs());

        registry.rebind_route(
            &Method::GET,
            "/v1/openapi.json",
            &OpenApiRouteConfig::default().with_doc("admin"),
        );

        let admin_doc = registry.document_by_name("admin").expect("admin document");
        assert!(!admin_doc.paths.contains_key("/v1/openapi.json"));
    }
}