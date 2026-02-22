//! OpenAPI registry and configuration.

use std::{collections::HashMap, sync::Arc};
use crate::{App, http::Method, headers::{Header, HttpHeaders, CacheControl, ETag}};
use volga_open_api::ui_html;

pub use volga_open_api::{
    OpenApiConfig, 
    OpenApiRegistry, 
    OpenApiSpec, 
    OpenApiRouteConfig,
    OpenApiDocument
};

pub(super) const OPEN_API_NOT_EXPOSED_WARN: &str = "OpenAPI configured but endpoints not exposed; call app.use_open_api() to serve spec/UI.";

#[derive(Debug, Default)]
pub(super) struct OpenApiState {
    pub(super) registry: Option<OpenApiRegistry>,
    pub(super) config: Option<OpenApiConfig>,
    pub(super) route_configs: HashMap<RouteKey, OpenApiRouteConfig>,
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub(super) struct RouteKey {
    pub(super) method: Method,
    pub(super) pattern: Arc<str>,
}

impl OpenApiState {
    /// Returns `true` if OpenAPI endpoints were exposed
    #[inline]
    pub(super) fn is_configure_but_not_exposed(&self) -> bool {
        self.config
            .as_ref()
            .is_some_and(|cfg| !cfg.exposed)
    }

    /// Updates OpenAPI configuration for the route
    #[inline]
    pub(super) fn update_route_config<T>(&mut self, key: &RouteKey, config: T)
    where
        T: FnOnce(OpenApiRouteConfig) -> OpenApiRouteConfig,
    {
        let entry = self
            .route_configs
            .get_mut(key)
            .expect("route config missing");

        let current = std::mem::take(entry);
        let updated = config(current);
        *entry = updated;

        if let Some(registry) = self.registry.as_ref() {
            registry.rebind_route(&key.method, &key.pattern, entry);
        }
    }

    /// Applies new route registration
    #[inline]
    pub(super) fn on_route_mapped(
        &mut self,
        key: RouteKey,
        auto: OpenApiRouteConfig
    ) {
        if let Some(entry) = self.route_configs.get_mut(&key) {
            *entry = auto;

            if let Some(registry) = self.registry.as_ref() {
                registry.rebind_route(&key.method, &key.pattern, entry);
            }
            return;
        }

        if let Some(reg) = self.registry.as_ref() {
            reg.register_route(&key.method, &key.pattern, &auto);
            reg.apply_route_config(&key.method, &key.pattern, &auto);
        }

        self.route_configs.insert(key, auto);
    }

    /// Rebuilds the current registry from stored route configs.
    #[inline]
    fn replay_all_routes_to_registry(&mut self) {
        let Some(registry) = &self.registry else {
            return;
        };

        for (key, cfg) in &self.route_configs {
            registry.register_route(&key.method, &key.pattern, cfg);
            registry.apply_route_config(&key.method, &key.pattern, cfg);
        }
    }
}

impl App {
    /// Configures OpenAPI registry with custom settings.
    ///
    /// # Example
    /// ```no_run
    /// use volga::App;
    ///
    /// let app = App::new()
    ///     .with_open_api(|config| config
    ///         .with_title("Example API")
    ///         .with_version("1.0.0"));
    /// ```
    pub fn with_open_api<T>(mut self, config: T) -> Self
    where
        T: FnOnce(OpenApiConfig) -> OpenApiConfig,
    {
        let config = config(self.openapi.config.unwrap_or_default());
        let registry = OpenApiRegistry::new(config.clone());
        
        self.openapi.config = Some(config);
        self.openapi.registry = Some(registry);
        self.openapi.replay_all_routes_to_registry();
        self
    }

    /// Sets OpenAPI registry with the provided configuration.
    pub fn set_open_api(mut self, config: OpenApiConfig) -> Self {
        self.openapi.registry = Some(OpenApiRegistry::new(config.clone()));
        self.openapi.config = Some(config);
        self.openapi.replay_all_routes_to_registry();
        self
    }

    /// Registers the OpenAPI JSON endpoint.
    pub fn use_open_api(&mut self) -> &mut Self {
        let (Some(registry), Some(config)) = (self.openapi.registry.clone(), &mut self.openapi.config) else {
            panic!(
                "OpenAPI is not configured. Use `App::with_open_api` or `App::set_open_api` to configure it."
            );
        };
        
        config.exposed = true;
        
        let config = config.clone();
        let cache_control = create_spec_cache_control();
        for spec in registry.specs().to_vec() {
            let registry = registry.clone();
            let cache_control = cache_control.clone();
            
            self.map_get(&spec.spec_path, move || {
                let spec_name = spec.name.clone();
                let registry = registry.clone();
                let cache_control = cache_control.clone();
                
                async move {
                    let Some(doc) = registry.document_by_name(&spec_name) else {
                        return crate::status!(404);
                    };
                    
                    crate::ok!(doc; [cache_control])
                }
            });
        }

        if config.ui_enabled() {
            let html = ui_html(registry.specs(), config.title());
            let etag = create_etag(html.as_bytes());
            let cache_control = create_ui_cache_control();
            
            self.map_get(config.ui_path(), move |headers: HttpHeaders| {
                let etag = etag.clone();
                let cache_control = cache_control.clone();
                let html = html.clone();
                
                async move {
                    if crate::headers::helpers::validate_etag(&etag, &headers) {
                        return crate::status!(304; [Header::<ETag>::try_from(etag)?]);
                    }
                    
                    crate::html!(html; [
                        cache_control, 
                        Header::<ETag>::try_from(etag)?
                    ])
                }
            });
        }

        self
    }
}

fn create_spec_cache_control() -> Header<CacheControl> {
    Header::try_from(
        CacheControl::default()
            .with_public()
            .with_max_age(60)
            .with_stale_while_revalidate(600))
        .expect("invalid cache control header")
}

fn create_ui_cache_control() -> Header<CacheControl> {
    Header::try_from(
        CacheControl::default()
            .with_public()
            .with_max_age(3600)
            .with_stale_while_revalidate(86400))
        .expect("invalid cache control header")
}

fn create_etag(bytes: &[u8]) -> ETag {
    use sha1::{Sha1, Digest};
    
    let mut hasher = Sha1::new();
    hasher.update(bytes);

    let tag = format!("{:x}", hasher.finalize());
    ETag::weak(tag)
}

#[cfg(test)]
mod tests {
    use serde_json::Value;

    use super::{
        OPEN_API_NOT_EXPOSED_WARN, OpenApiConfig, OpenApiRegistry, OpenApiSpec, OpenApiState,
        RouteKey, create_etag, create_spec_cache_control, create_ui_cache_control,
    };
    use crate::http::Method;

    #[test]
    fn exposed_warning_message_is_stable() {
        assert_eq!(
            OPEN_API_NOT_EXPOSED_WARN,
            "OpenAPI configured but endpoints not exposed; call app.use_open_api() to serve spec/UI.",
        );
    }

    #[test]
    fn spec_cache_control_has_short_ttl() {
        let header = create_spec_cache_control();
        assert_eq!(
            header.as_str().expect("cache control"),
            "max-age=60, public, stale-while-revalidate=600"
        );
    }

    #[test]
    fn ui_cache_control_has_longer_ttl() {
        let header = create_ui_cache_control();
        assert_eq!(
            header.as_str().expect("cache control"),
            "max-age=3600, public, stale-while-revalidate=86400"
        );
    }

    #[test]
    fn etag_is_deterministic_and_weak() {
        let first = create_etag(b"openapi");
        let second = create_etag(b"openapi");

        assert_eq!(first, second);
        assert!(first.is_weak());
        assert!(first.as_ref().starts_with("W/\""));
        assert!(first.as_ref().ends_with("\""));
        assert_eq!(first.tag().len(), 40);
    }

    #[test]
    fn remapping_existing_route_refreshes_auto_openapi_config() {
        let config = OpenApiConfig::new().with_specs([OpenApiSpec::new("v1")]);
        let registry = OpenApiRegistry::new(config.clone());

        let mut state = OpenApiState {
            registry: Some(registry.clone()),
            config: Some(config),
            ..Default::default()
        };

        let key = RouteKey {
            method: Method::GET,
            pattern: "/users".into(),
        };

        state.on_route_mapped(
            key.clone(),
            super::OpenApiRouteConfig::default().produces_text(),
        );
        state.on_route_mapped(
            key.clone(),
            super::OpenApiRouteConfig::default().produces_empty_json(),
        );

        let doc = registry.document_by_name("v1").expect("document");
        let json = serde_json::to_value(doc).expect("serialize openapi doc");

        assert_eq!(
            json["paths"]["/users"]["get"]["responses"]["200"]["content"]["application/json"]["schema"]["type"],
            Value::String("object".to_string())
        );
        assert!(json["paths"]["/users"]["get"]["responses"]["200"]["content"]
            .get("text/plain; charset=utf-8")
            .is_none());
    }

    #[test]
    fn replacing_registry_replays_existing_route_configs() {
        let config = OpenApiConfig::new().with_specs([OpenApiSpec::new("v1")]);
        let first_registry = OpenApiRegistry::new(config.clone());
        let replacement_registry = OpenApiRegistry::new(config.clone());

        let mut state = OpenApiState {
            registry: Some(first_registry.clone()),
            config: Some(config),
            ..Default::default()
        };

        let key = RouteKey {
            method: Method::GET,
            pattern: "/users".into(),
        };

        state.on_route_mapped(
            key,
            super::OpenApiRouteConfig::default().produces_text(),
        );

        let before = replacement_registry.document_by_name("v1").expect("document");
        let before_json = serde_json::to_value(before).expect("serialize");
        assert!(before_json["paths"].get("/users").is_none());

        state.registry = Some(replacement_registry.clone());
        state.replay_all_routes_to_registry();

        let after = replacement_registry.document_by_name("v1").expect("document");
        let after_json = serde_json::to_value(after).expect("serialize");
        assert!(after_json["paths"].get("/users").is_some());
    }
}
