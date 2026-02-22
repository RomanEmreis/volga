//! Types and utils for the OpenAPI registry.

use std::{collections::{BTreeMap, HashSet}, sync::{Arc, Mutex}};
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
        validate_specs(&config.specs);

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

    /// Registers a route in the OpenAPI registry.
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

        let mut valid_targets: Vec<&str> = targets
            .into_iter()
            .filter(|name| docs.contains_key(*name))
            .collect();

        // If user specified docs but none exist -> do not drop the route from all specs.
        if valid_targets.is_empty() {
            #[cfg(debug_assertions)]
            {
                eprintln!(
                    "OpenAPI: rebind_route ignored: no such spec(s) for route {method_lc} {spec_path}"
                );
            }
            return;
        }

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

        for name in valid_targets.drain(..) {
            let Some(doc) = docs.get_mut(name) else { 
                continue;
            };

            cfg.apply_to_operation(&mut op, &mut doc.components.schemas);

            doc.paths
                .entry(spec_path.clone())
                .or_default()
                .insert(method_lc.clone(), op.clone());
        }

        for doc in docs.values_mut() {
            doc.prune_unreferenced_components();
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
            let op = entry
                .entry(method_lc.clone())
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

    #[inline]
    fn is_excluded_path(&self, path: &str) -> bool {
        let p = normalize_path_for_compare(path);

        if self.ui_enabled && p == normalize_path_for_compare(&self.ui_path) {
            return true;
        }

        self.specs
            .iter()
            .any(|s| p == normalize_path_for_compare(&s.spec_path))
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

fn validate_specs(specs: &[OpenApiSpec]) {
    let mut names = HashSet::with_capacity(specs.len());
    let mut paths = HashSet::with_capacity(specs.len());

    for spec in specs {
        assert!(
            names.insert(spec.name.clone()),
            "OpenAPI spec names must be unique; duplicate name: {}",
            spec.name
        );

        let normalized_path = normalize_path_for_compare(&spec.spec_path);
        assert!(
            paths.insert(normalized_path.clone()),
            "OpenAPI spec paths must be unique after normalization; duplicate path: {normalized_path}"
        );
    }
}

#[inline]
fn normalize_path_for_compare(p: &str) -> String {
    // treat "" as "/"
    let p = if p.is_empty() { 
        "/" 
    } else { 
        p
    };

    // ensure leading slash
    let p = if p.starts_with('/') {
        p.to_string()
    } else {
        format!("/{p}")
    };

    // drop trailing slash (except root)
    if p.len() > 1 && p.ends_with('/') {
        p.trim_end_matches('/').to_string()
    } else {
        p
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

    #[test]
    fn rebind_route_prunes_unreferenced_component_schemas() {
        let registry = OpenApiRegistry::new(OpenApiConfig::new().with_specs([OpenApiSpec::new("v1")]));

        let first = OpenApiRouteConfig::default().with_response_schema(
            crate::schema::OpenApiSchema::object()
                .with_title("User")
                .with_property("name", crate::schema::OpenApiSchema::string()),
        );
        let second = OpenApiRouteConfig::default().with_response_schema(
            crate::schema::OpenApiSchema::object()
                .with_title("User")
                .with_property("id", crate::schema::OpenApiSchema::integer()),
        );

        registry.register_route(&Method::GET, "/users", &first);
        registry.apply_route_config(&Method::GET, "/users", &first);

        registry.rebind_route(&Method::GET, "/users", &second);

        let v1_doc = registry.document_by_name("v1").expect("v1 document");

        assert_eq!(v1_doc.components.schemas.len(), 1);
        assert!(!v1_doc.components.schemas.contains_key("User"));
        assert!(v1_doc.components.schemas.contains_key("User_2"));
    }
    
    #[test]
    fn it_tests_normalization_for_excluded_paths() {
        assert_eq!(normalize_path_for_compare("/v1/openapi.json").as_str(), "/v1/openapi.json");
        assert_eq!(normalize_path_for_compare("openapi"), "/openapi");
        assert_eq!(normalize_path_for_compare(""), "/");
        assert_eq!(normalize_path_for_compare("/openapi/"), "/openapi");
    }

    #[test]
    #[should_panic(expected = "OpenAPI spec names must be unique")]
    fn registry_new_panics_on_duplicate_spec_names() {
        let config = OpenApiConfig::new().with_specs([
            OpenApiSpec::new("v1"),
            OpenApiSpec::new("v1").with_path("/v1-alt/openapi.json"),
        ]);

        let _ = OpenApiRegistry::new(config);
    }

    #[test]
    #[should_panic(expected = "OpenAPI spec paths must be unique after normalization")]
    fn registry_new_panics_on_duplicate_normalized_spec_paths() {
        let config = OpenApiConfig::new().with_specs([
            OpenApiSpec::new("v1").with_path("docs/openapi.json"),
            OpenApiSpec::new("admin").with_path("/docs/openapi.json/"),
        ]);

        let _ = OpenApiRegistry::new(config);
    }
}
