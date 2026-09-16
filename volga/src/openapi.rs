//! OpenAPI registry and configuration.

use crate::{
    App,
    headers::{CacheControl, ETag, Header, HttpHeaders},
    http::{
        Method,
        endpoints::route::{is_catch_all_segment, is_dynamic_segment, split_path},
    },
};
use std::{collections::HashMap, sync::Arc};
use volga_open_api::ui_html;

pub use volga_open_api::{
    ConstraintTarget, FieldConstraint, OpenApiConfig, OpenApiDocument, OpenApiRegistry,
    OpenApiRouteConfig, OpenApiSpec, SchemaConstraint,
};

pub(super) const OPEN_API_NOT_EXPOSED_WARN: &str =
    "OpenAPI configured but endpoints not exposed; call app.use_open_api() to serve spec/UI.";

/// Reports a catch-all route left out of the OpenAPI document, and the route describing its
/// position instead
#[cfg_attr(not(debug_assertions), allow(dead_code))]
pub(super) fn undescribed_catch_all_warning(catch_all: &RouteKey, by: &RouteKey) -> String {
    format!(
        "OpenAPI: `{} {}` is left out of the document. OpenAPI describes a catch-all as a \
         one-segment path parameter, which `{} {}` already is, and one templated path cannot \
         carry two operations for one method.",
        catch_all.method, catch_all.pattern, by.method, by.pattern
    )
}

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

impl RouteKey {
    /// Returns `true` when this route ends in a catch-all parameter
    #[inline]
    fn is_catch_all(&self) -> bool {
        split_path(&self.pattern)
            .last()
            .is_some_and(is_catch_all_segment)
    }

    /// Returns `true` when this route is a catch-all that `other` - a route for the same
    /// method with a parameter at the catch-all's position - takes the OpenAPI operation
    /// from.
    ///
    /// OpenAPI templates a path one segment at a time, so a catch-all is described as the
    /// parameter it would be in one segment, and the two routes are one templated path
    /// there: an operation of one would be merged into the other's, or overwritten by it.
    /// The parameter route is the one a spec can describe faithfully, so it is the one kept.
    #[inline]
    fn is_shadowed_by(&self, other: &RouteKey) -> bool {
        self.method == other.method
            && self.is_catch_all()
            && !other.is_catch_all()
            && is_same_template(&self.pattern, &other.pattern)
    }
}

/// Returns `true` when two route patterns spell one OpenAPI path template: the same number
/// of segments, with a literal wherever the other has that literal and a parameter wherever
/// the other has any parameter.
#[inline]
fn is_same_template(left: &str, right: &str) -> bool {
    let mut left = split_path(left);
    let mut right = split_path(right);

    loop {
        match (left.next(), right.next()) {
            (None, None) => return true,
            (Some(l), Some(r)) => {
                let same = match (is_dynamic_segment(l), is_dynamic_segment(r)) {
                    (true, true) => true,
                    (false, false) => l == r,
                    _ => false,
                };

                if !same {
                    return false;
                }
            }
            _ => return false,
        }
    }
}

impl OpenApiState {
    /// Returns `true` if OpenAPI endpoints were exposed
    #[inline]
    pub(super) fn is_configure_but_not_exposed(&self) -> bool {
        self.config.as_ref().is_some_and(|cfg| !cfg.exposed)
    }

    /// Returns the route that keeps `key` out of the OpenAPI document, if any.
    /// See [`RouteKey::is_shadowed_by`].
    #[inline]
    fn shadowing(&self, key: &RouteKey) -> Option<&RouteKey> {
        if !key.is_catch_all() {
            return None;
        }

        self.route_configs
            .keys()
            .find(|other| key.is_shadowed_by(other))
    }

    /// The catch-all routes left out of the OpenAPI document, each with the route whose
    /// operation takes its place - empty unless OpenAPI is configured.
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    pub(super) fn undescribed_catch_alls(&self) -> Vec<(&RouteKey, &RouteKey)> {
        if self.registry.is_none() {
            return Vec::new();
        }

        let mut undescribed: Vec<_> = self
            .route_configs
            .keys()
            .filter_map(|key| self.shadowing(key).map(|by| (key, by)))
            .collect();

        undescribed.sort_by(|(left, _), (right, _)| {
            (left.pattern.as_ref(), left.method.as_str())
                .cmp(&(right.pattern.as_ref(), right.method.as_str()))
        });
        undescribed
    }

    /// Updates OpenAPI configuration for the route
    #[inline]
    pub(super) fn update_route_config<T>(&mut self, key: &RouteKey, config: T)
    where
        T: FnOnce(OpenApiRouteConfig) -> OpenApiRouteConfig,
    {
        let described = self.shadowing(key).is_none();
        let entry = self
            .route_configs
            .get_mut(key)
            .expect("route config missing");

        let current = std::mem::take(entry);
        let updated = config(current);
        *entry = updated;

        if described && let Some(registry) = self.registry.as_ref() {
            registry.rebind_route(&key.method, &key.pattern, entry);
        }
    }

    /// Applies new route registration
    #[inline]
    pub(super) fn on_route_mapped(&mut self, key: RouteKey, auto: OpenApiRouteConfig) {
        // A route taking a described catch-all's position takes its operation as well, so
        // the catch-all's operation goes first - otherwise this route's would be merged into
        // it rather than written on its own
        if let Some(registry) = self.registry.as_ref() {
            self.route_configs
                .keys()
                .filter(|other| other.is_shadowed_by(&key) && self.shadowing(other).is_none())
                .for_each(|catch_all| registry.remove_route(&catch_all.method, &catch_all.pattern));
        }

        let described = self.shadowing(&key).is_none();

        if let Some(entry) = self.route_configs.get_mut(&key) {
            *entry = auto;

            if described && let Some(registry) = self.registry.as_ref() {
                registry.rebind_route(&key.method, &key.pattern, entry);
            }
            return;
        }

        if described && let Some(reg) = self.registry.as_ref() {
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
            if self.shadowing(key).is_some() {
                continue;
            }

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
        let (Some(registry), Some(config)) =
            (self.openapi.registry.clone(), &mut self.openapi.config)
        else {
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
                    if crate::headers::helpers::validate_etag(&etag, headers.as_map()) {
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
            .with_stale_while_revalidate(600),
    )
    .expect("invalid cache control header")
}

fn create_ui_cache_control() -> Header<CacheControl> {
    Header::try_from(
        CacheControl::default()
            .with_public()
            .with_max_age(3600)
            .with_stale_while_revalidate(86400),
    )
    .expect("invalid cache control header")
}

fn create_etag(bytes: &[u8]) -> ETag {
    use crate::utils::lower_hex;
    use sha1::{Digest, Sha1};

    let mut hasher = Sha1::new();
    hasher.update(bytes);

    let tag = lower_hex(&hasher.finalize());
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
            super::OpenApiRouteConfig::default().produces_text(200u16),
        );
        state.on_route_mapped(
            key.clone(),
            super::OpenApiRouteConfig::default().produces_empty_json(200u16),
        );

        let doc = registry.document_by_name("v1").expect("document");
        let json = serde_json::to_value(doc).expect("serialize openapi doc");

        assert_eq!(
            json["paths"]["/users"]["get"]["responses"]["200"]["content"]["application/json"]["schema"]
                ["type"],
            Value::String("object".to_string())
        );
        assert!(
            json["paths"]["/users"]["get"]["responses"]["200"]["content"]
                .get("text/plain; charset=utf-8")
                .is_none()
        );
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
            super::OpenApiRouteConfig::default().produces_text(200u16),
        );

        let before = replacement_registry
            .document_by_name("v1")
            .expect("document");
        let before_json = serde_json::to_value(before).expect("serialize");
        assert!(before_json["paths"].get("/users").is_none());

        state.registry = Some(replacement_registry.clone());
        state.replay_all_routes_to_registry();

        let after = replacement_registry
            .document_by_name("v1")
            .expect("document");
        let after_json = serde_json::to_value(after).expect("serialize");
        assert!(after_json["paths"].get("/users").is_some());
    }

    /// An `OpenApiState` with a registry for one spec, `v1`.
    fn configured_state() -> (OpenApiState, OpenApiRegistry) {
        let config = OpenApiConfig::new().with_specs([OpenApiSpec::new("v1")]);
        let registry = OpenApiRegistry::new(config.clone());

        let state = OpenApiState {
            registry: Some(registry.clone()),
            config: Some(config),
            ..Default::default()
        };

        (state, registry)
    }

    fn key(method: Method, pattern: &str) -> RouteKey {
        RouteKey {
            method,
            pattern: pattern.into(),
        }
    }

    /// Maps `pattern` for `method` with a summary naming it.
    fn map(state: &mut OpenApiState, method: Method, pattern: &str, summary: &str) {
        let key = key(method, pattern);
        state.on_route_mapped(key.clone(), super::OpenApiRouteConfig::default());
        state.update_route_config(&key, |cfg| cfg.with_summary(summary));
    }

    fn paths(registry: &OpenApiRegistry) -> Value {
        let doc = registry.document_by_name("v1").expect("document");
        serde_json::to_value(doc).expect("serialize openapi doc")["paths"].clone()
    }

    /// A catch-all and a parameter route at one position are one templated path in the
    /// document, so only the parameter route is described - whichever was mapped first
    #[test]
    fn it_leaves_out_a_catch_all_beside_a_parameter_route_in_any_order() {
        for catch_all_first in [true, false] {
            let (mut state, registry) = configured_state();

            if catch_all_first {
                map(&mut state, Method::GET, "/files/{*path}", "the rest");
                map(&mut state, Method::GET, "/files/{path}", "one segment");
            } else {
                map(&mut state, Method::GET, "/files/{path}", "one segment");
                map(&mut state, Method::GET, "/files/{*path}", "the rest");
            }

            let paths = paths(&registry);
            assert_eq!(
                paths["/files/{path}"]["get"]["summary"],
                Value::String("one segment".into()),
                "catch-all first: {catch_all_first}"
            );
            assert_eq!(paths.as_object().unwrap().len(), 1);
        }
    }

    #[test]
    fn it_keeps_a_parameter_route_described_when_the_catch_all_beside_it_is_remapped() {
        let (mut state, registry) = configured_state();

        map(&mut state, Method::GET, "/files/{path}", "one segment");
        map(&mut state, Method::GET, "/files/{*path}", "the rest");
        state.on_route_mapped(
            key(Method::GET, "/files/{*path}"),
            super::OpenApiRouteConfig::default(),
        );

        assert_eq!(
            paths(&registry)["/files/{path}"]["get"]["summary"],
            Value::String("one segment".into())
        );
    }

    /// The position decides, not the name: two differently named parameters are still one
    /// templated path
    #[test]
    fn it_leaves_out_a_catch_all_beside_a_parameter_route_named_otherwise() {
        let (mut state, registry) = configured_state();

        map(&mut state, Method::GET, "/files/{*path}", "the rest");
        map(&mut state, Method::GET, "/files/{id}", "one segment");

        let paths = paths(&registry);
        assert!(paths.get("/files/{path}").is_none());
        assert_eq!(
            paths["/files/{id}"]["get"]["summary"],
            Value::String("one segment".into())
        );
    }

    #[test]
    fn it_describes_a_catch_all_beside_a_route_on_another_verb_or_a_literal() {
        let (mut state, registry) = configured_state();

        map(&mut state, Method::POST, "/files/{path}", "upload");
        map(&mut state, Method::GET, "/files/meta", "meta");
        map(&mut state, Method::GET, "/files/{*path}", "the rest");

        let paths = paths(&registry);
        assert_eq!(
            paths["/files/{path}"]["get"]["summary"],
            Value::String("the rest".into())
        );
        assert_eq!(
            paths["/files/{path}"]["post"]["summary"],
            Value::String("upload".into())
        );
        assert!(paths["/files/meta"].get("get").is_some());
    }

    /// Routes mapped before OpenAPI is configured are replayed into the registry the same way
    #[test]
    fn it_leaves_out_a_catch_all_when_replaying_routes() {
        let mut state = OpenApiState::default();

        map(&mut state, Method::GET, "/files/{*path}", "the rest");
        map(&mut state, Method::GET, "/files/{path}", "one segment");

        let config = OpenApiConfig::new().with_specs([OpenApiSpec::new("v1")]);
        let registry = OpenApiRegistry::new(config.clone());
        state.registry = Some(registry.clone());
        state.config = Some(config);
        state.replay_all_routes_to_registry();

        let paths = paths(&registry);
        assert_eq!(paths.as_object().unwrap().len(), 1);
        assert_eq!(
            paths["/files/{path}"]["get"]["summary"],
            Value::String("one segment".into())
        );
    }

    #[test]
    fn it_names_the_catch_alls_it_leaves_out() {
        let (mut state, _) = configured_state();

        map(&mut state, Method::GET, "/files/{*path}", "the rest");
        map(&mut state, Method::GET, "/files/{path}", "one segment");
        map(&mut state, Method::GET, "/users/{*rest}", "users");

        let undescribed = state.undescribed_catch_alls();
        assert_eq!(undescribed.len(), 1);

        let (catch_all, by) = undescribed[0];
        assert_eq!(
            super::undescribed_catch_all_warning(catch_all, by),
            "OpenAPI: `GET /files/{*path}` is left out of the document. OpenAPI describes a \
             catch-all as a one-segment path parameter, which `GET /files/{path}` already is, \
             and one templated path cannot carry two operations for one method."
        );

        // Nothing is left out of a document that does not exist
        assert!(OpenApiState::default().undescribed_catch_alls().is_empty());
    }

    /// OpenAPI templates a path one segment at a time, so a catch-all is described as the
    /// path parameter closest to it
    #[test]
    fn it_describes_a_catch_all_route_as_a_path_parameter() {
        let config = OpenApiConfig::new().with_specs([OpenApiSpec::new("v1")]);
        let registry = OpenApiRegistry::new(config.clone());

        let mut state = OpenApiState {
            registry: Some(registry.clone()),
            config: Some(config),
            ..Default::default()
        };

        state.on_route_mapped(
            RouteKey {
                method: Method::GET,
                pattern: "/files/{*path}".into(),
            },
            super::OpenApiRouteConfig::default().produces_text(200u16),
        );

        let doc = registry.document_by_name("v1").expect("document");
        let json = serde_json::to_value(doc).expect("serialize openapi doc");
        let parameters = &json["paths"]["/files/{path}"]["get"]["parameters"];

        assert_eq!(parameters[0]["name"], Value::String("path".to_string()));
        assert_eq!(parameters[0]["in"], Value::String("path".to_string()));
        assert!(json["paths"].get("/files/{*path}").is_none());
    }
}
