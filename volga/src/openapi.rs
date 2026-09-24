//! OpenAPI registry and configuration.

use crate::{
    App,
    headers::{CacheControl, ETag, Header, HttpHeaders},
    http::{
        Method,
        endpoints::route::{
            is_catch_all_segment, is_dynamic_segment, param_name, split_path, untyped_path,
        },
    },
};
use std::{
    borrow::Cow,
    cmp::Reverse,
    collections::{BTreeMap, HashMap},
    sync::Arc,
};
use volga_open_api::ui_html;

pub use volga_open_api::{
    ConstraintTarget, FieldConstraint, InputKind, OpenApiConfig, OpenApiDocument, OpenApiRegistry,
    OpenApiRouteConfig, OpenApiSchema, OpenApiSpec, SchemaConstraint, UndescribedInput,
};

pub(super) const OPEN_API_NOT_EXPOSED_WARN: &str =
    "OpenAPI configured but endpoints not exposed; call app.use_open_api() to serve spec/UI.";

/// Reports a catch-all route left out of OpenAPI documents, and the route describing its
/// position there instead
#[cfg_attr(not(debug_assertions), allow(dead_code))]
pub(super) fn undescribed_catch_all_warning(
    catch_all: &RouteKey,
    by: &RouteKey,
    docs: &[&str],
) -> String {
    let names = docs
        .iter()
        .map(|doc| format!("`{doc}`"))
        .collect::<Vec<_>>()
        .join(", ");

    let there = if docs.len() == 1 {
        "that document"
    } else {
        "those documents"
    };

    format!(
        "OpenAPI: `{} {}` is left out of {names}. OpenAPI describes a catch-all as a \
         one-segment path parameter, which `{} {}` already is in {there}, and one templated \
         path cannot carry two operations for one method.",
        catch_all.method, catch_all.pattern, by.method, by.pattern
    )
}

/// Reports a route OpenAPI documents describe under the parameter names of other routes at
/// its position, and the names it is described under
#[cfg_attr(not(debug_assertions), allow(dead_code))]
pub(super) fn renamed_route_warning(
    route: &RouteKey,
    template: &RouteKey,
    docs: &[&str],
) -> String {
    let names = docs
        .iter()
        .map(|doc| format!("`{doc}`"))
        .collect::<Vec<_>>()
        .join(", ");

    let renames = route
        .param_names()
        .into_iter()
        .zip(template.param_names())
        .filter(|(own, to)| own != to)
        .map(|(own, to)| format!("`{own}` as `{to}`"))
        .collect::<Vec<_>>()
        .join(", ");

    format!(
        "OpenAPI: `{} {}` is described as `{}` in {names}, with {renames}. A templated path \
         takes one set of parameter names, so the routes at one position are described under \
         the names most of them are written with, the first in alphabetical order on a tie. \
         Naming the parameters alike describes each route under its own.",
        route.method,
        route.pattern,
        spell_template(&template.pattern, param_name)
    )
}

/// Reports a handler input an OpenAPI document describes without its fields, and how to
/// describe it by hand
#[cfg_attr(not(debug_assertions), allow(dead_code))]
pub(super) fn undescribed_input_warning(route: &RouteKey, input: &UndescribedInput) -> String {
    let type_name = short_type_name(input.type_name());

    let (described, fix) = match input.kind() {
        InputKind::QueryParameters => (
            format!("none of the query parameters of `{type_name}`"),
            "Describe them by hand with `.open_api(|c| c.with_query_schema(..))`.",
        ),
        _ => {
            let shape = if input.read_as_map().is_some() {
                "any value"
            } else {
                "an object without any fields"
            };
            (
                format!("its request body `{type_name}` as {shape}"),
                "Describe the body by hand with `.open_api(|c| c.with_request_schema(..))`.",
            )
        }
    };

    let read_as_map = match input.read_as_map() {
        Some(expecting) => format!("`{expecting}` inside it"),
        None => format!("`{type_name}`"),
    };

    format!(
        "OpenAPI: `{} {}` describes {described}: serde reads {read_as_map} as a map, as it does \
         a struct with a `#[serde(flatten)]` field, and a map does not name the keys it takes. \
         {fix}",
        route.method, route.pattern
    )
}

/// Spells a type the way code in scope of it does: `alloc::vec::Vec<app::Flat>` as
/// `Vec<Flat>`
#[cfg_attr(not(debug_assertions), allow(dead_code))]
fn short_type_name(type_name: &str) -> String {
    let mut out = String::with_capacity(type_name.len());
    // Where the path being read started, so that its module prefix can be dropped
    let mut path_start = 0;
    let mut chars = type_name.chars().peekable();

    while let Some(c) = chars.next() {
        if c == ':' && chars.peek() == Some(&':') {
            chars.next();
            out.truncate(path_start);
        } else {
            out.push(c);
            if !(c.is_alphanumeric() || c == '_') {
                path_start = out.len();
            }
        }
    }
    out
}

#[derive(Debug, Default)]
pub(super) struct OpenApiState {
    pub(super) registry: Option<OpenApiRegistry>,
    pub(super) config: Option<OpenApiConfig>,
    pub(super) route_configs: HashMap<RouteKey, OpenApiRouteConfig>,
    /// The routes mapped at each position of an OpenAPI path template - see
    /// [`RouteKey::position`] - which a document describes together. A route alone at its
    /// position is written to the registry on its own.
    positions: HashMap<Box<str>, Vec<RouteKey>>,
    /// The spelling each mapped route was last mapped under, keyed by the route it names -
    /// see [`RouteKey::untyped`] - so that mapping a route again under another spelling finds
    /// the configuration it replaces.
    spellings: HashMap<RouteKey, RouteKey>,
}

#[derive(Debug, Clone, Hash, Eq, PartialEq)]
pub(super) struct RouteKey {
    pub(super) method: Method,
    pub(super) pattern: Arc<str>,
}

impl RouteKey {
    /// This route as the router reads it, without the type annotations of its parameters:
    /// `GET /users/{id:integer}` and `GET /users/{id}` are one route
    #[inline]
    fn untyped(&self) -> RouteKey {
        match untyped_path(&self.pattern) {
            Cow::Borrowed(_) => self.clone(),
            Cow::Owned(pattern) => RouteKey {
                method: self.method.clone(),
                pattern: pattern.into(),
            },
        }
    }

    /// Returns `true` when this route ends in a catch-all parameter
    #[inline]
    fn is_catch_all(&self) -> bool {
        split_path(&self.pattern)
            .last()
            .is_some_and(is_catch_all_segment)
    }

    /// The position this route is at in an OpenAPI path template, whatever it calls its
    /// parameters: `/files/{name}` and `/files/{*path}` are both at `/files/{}`.
    ///
    /// OpenAPI templates a path one segment at a time, so a catch-all is described as the
    /// parameter it would be in one segment, and a document takes one templated path for a
    /// position - see [`plan_position`].
    #[inline]
    fn position(&self) -> Box<str> {
        spell_template(&self.pattern, |_| "")
    }

    /// The names this route's parameters are written with, in path order
    #[inline]
    fn param_names(&self) -> Vec<&str> {
        split_path(&self.pattern)
            .filter(|segment| is_dynamic_segment(segment))
            .map(param_name)
            .collect()
    }
}

/// Spells a route pattern the way OpenAPI templates it, with each parameter named `name`
/// calls it
#[inline]
fn spell_template(pattern: &str, name: impl Fn(&str) -> &str) -> Box<str> {
    let mut path = String::with_capacity(pattern.len());
    for segment in split_path(pattern) {
        path.push('/');
        if is_dynamic_segment(segment) {
            path.push('{');
            path.push_str(name(segment));
            path.push('}');
        } else {
            path.push_str(segment);
        }
    }

    if path.is_empty() {
        path.push('/');
    }

    path.into()
}

impl OpenApiState {
    /// Returns `true` if OpenAPI endpoints were exposed
    #[inline]
    pub(super) fn is_configure_but_not_exposed(&self) -> bool {
        self.config.as_ref().is_some_and(|cfg| !cfg.exposed)
    }

    /// The routes mapped at `key`'s position, `key` included once it is mapped - see
    /// [`RouteKey::position`].
    #[inline]
    fn position_routes(&self, key: &RouteKey) -> Vec<(&RouteKey, &OpenApiRouteConfig)> {
        self.positions
            .get(&key.position())
            .map(|keys| self.routes(keys))
            .unwrap_or_default()
    }

    /// The routes `keys` name, with their configurations
    #[inline]
    fn routes<'a>(&'a self, keys: &'a [RouteKey]) -> Vec<(&'a RouteKey, &'a OpenApiRouteConfig)> {
        keys.iter()
            .filter_map(|key| self.route_configs.get_key_value(key))
            .collect()
    }

    /// The routes sharing an OpenAPI operation with `key`, `key` included once it is mapped:
    /// the ones mapped for its method at its position.
    #[inline]
    fn operation_group(&self, key: &RouteKey) -> Vec<(&RouteKey, &OpenApiRouteConfig)> {
        let mut group = self.position_routes(key);
        group.retain(|(other, _)| other.method == key.method);
        group
    }

    /// The catch-all routes left out of OpenAPI documents, each with the route whose
    /// operation takes its place and the documents it is left out of - empty unless OpenAPI
    /// is configured.
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    pub(super) fn undescribed_catch_alls(&self) -> Vec<(&RouteKey, &RouteKey, Vec<&str>)> {
        let Some(registry) = self.registry.as_ref() else {
            return Vec::new();
        };

        let mut undescribed = Vec::new();

        for (key, cfg) in &self.route_configs {
            if !key.is_catch_all() {
                continue;
            }

            let mut routes = self.operation_group(key);
            routes.retain(|(other, _)| !other.is_catch_all());
            routes.sort_by(|(left, _), (right, _)| left.pattern.cmp(&right.pattern));

            let docs = placement(registry, cfg);
            if let Some((by, left_out)) = routes.iter().find_map(|(other, other_cfg)| {
                let theirs = placement(registry, other_cfg);
                let left_out: Vec<&str> = docs
                    .iter()
                    .copied()
                    .filter(|doc| theirs.contains(doc))
                    .collect();

                (!left_out.is_empty()).then_some((*other, left_out))
            }) {
                undescribed.push((key, by, left_out));
            }
        }

        undescribed.sort_by(|(left, _, _), (right, _, _)| {
            (left.pattern.as_ref(), left.method.as_str())
                .cmp(&(right.pattern.as_ref(), right.method.as_str()))
        });
        undescribed
    }

    /// The routes OpenAPI documents describe under the parameter names of other routes at
    /// their position, each with a route whose names it is described under and the
    /// documents it is described so in - empty unless OpenAPI is configured.
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    pub(super) fn renamed_routes(&self) -> Vec<(&RouteKey, &RouteKey, Vec<&str>)> {
        let Some(registry) = self.registry.as_ref() else {
            return Vec::new();
        };

        let mut renamed: Vec<(&RouteKey, &RouteKey, Vec<&str>)> = Vec::new();

        for keys in self.positions.values().filter(|keys| keys.len() > 1) {
            let routes = self.routes(keys);

            for plan in plan_position(registry, &routes) {
                let names = plan.template.param_names();

                for (key, _) in plan.routes {
                    if key.param_names() == names {
                        continue;
                    }

                    // One line for a route described alike in several documents
                    match renamed.iter_mut().find(|(route, template, _)| {
                        *route == key && template.param_names() == names
                    }) {
                        Some((_, _, docs)) => docs.push(plan.doc),
                        None => renamed.push((key, plan.template, vec![plan.doc])),
                    }
                }
            }
        }

        renamed.sort_by(|(left, _, _), (right, _, _)| {
            (left.pattern.as_ref(), left.method.as_str())
                .cmp(&(right.pattern.as_ref(), right.method.as_str()))
        });
        renamed
    }

    /// Returns `true` when some document describes `key` - see [`plan_position`]
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    fn is_described(&self, registry: &OpenApiRegistry, key: &RouteKey) -> bool {
        plan_position(registry, &self.position_routes(key))
            .iter()
            .any(|plan| plan.routes.iter().any(|(route, _)| *route == key))
    }

    /// The handler inputs OpenAPI documents describe without their fields, each with the
    /// route that reads it - empty unless OpenAPI is configured.
    #[cfg_attr(not(debug_assertions), allow(dead_code))]
    pub(super) fn undescribed_inputs(&self) -> Vec<(&RouteKey, &UndescribedInput)> {
        let Some(registry) = self.registry.as_ref() else {
            return Vec::new();
        };

        let mut undescribed: Vec<_> = self
            .route_configs
            .iter()
            .filter(|(key, cfg)| {
                // A route no document describes - a catch-all left out of every one of
                // its documents - describes none of its inputs either
                !cfg.undescribed_inputs().is_empty() && self.is_described(registry, key)
            })
            .flat_map(|(key, cfg)| {
                cfg.undescribed_inputs()
                    .iter()
                    .map(move |input| (key, input))
            })
            .collect();

        // Sorted by route alone, so that the inputs of one route keep the order its
        // handler reads them in
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
        // A route mapped again under another spelling was replaced, configuration and all -
        // a group closing over both spellings reaches the one that is gone as well
        let Some(entry) = self.route_configs.get_mut(key) else {
            return;
        };

        let current = std::mem::take(entry);
        let updated = config(current);
        *entry = updated;

        self.write_route(key, true);
    }

    /// Applies new route registration
    #[inline]
    pub(super) fn on_route_mapped(&mut self, key: RouteKey, auto: OpenApiRouteConfig) {
        // Mapping a handler where one is mapped replaces the route, and a type annotation is
        // not part of what the router reads - so a route mapped again under another spelling
        // replaces the configuration of the one it was mapped as, rather than leaving it to be
        // merged into this one's operation
        let replaced = self
            .spellings
            .insert(key.untyped(), key.clone())
            .filter(|previous| *previous != key)
            .and_then(|previous| self.route_configs.remove_entry(&previous));

        if let Some((previous, _)) = &replaced {
            if let Some(keys) = self.positions.get_mut(&previous.position()) {
                keys.retain(|other| other != previous);
            }

            if let Some(registry) = self.registry.as_ref() {
                registry.remove_route(&previous.method, &previous.pattern);
            }
        }

        let keys = self.positions.entry(key.position()).or_default();
        if !keys.contains(&key) {
            keys.push(key.clone());
        }

        let remapped = self.route_configs.insert(key.clone(), auto).is_some() || replaced.is_some();
        self.write_route(&key, remapped);
    }

    /// Writes a mapped route's configuration to the registry, if one is configured.
    ///
    /// A route sharing its position with others rewrites every route there, since the
    /// documents that describe each of them, and the names they are described under, depend
    /// on all of their configurations at once - see [`plan_position`]. A route alone at its
    /// position is written on its own, as it always was.
    #[inline]
    fn write_route(&self, key: &RouteKey, remapped: bool) {
        let Some(registry) = self.registry.as_ref() else {
            return;
        };

        let routes = self.position_routes(key);
        if routes.len() > 1 {
            describe_position(registry, &routes);
            return;
        }

        let Some(cfg) = self.route_configs.get(key) else {
            return;
        };

        if remapped {
            registry.rebind_route(&key.method, &key.pattern, cfg);
        } else {
            registry.register_route(&key.method, &key.pattern, cfg);
            registry.apply_route_config(&key.method, &key.pattern, cfg);
        }
    }

    /// Rebuilds the current registry from stored route configs.
    #[inline]
    fn replay_all_routes_to_registry(&mut self) {
        let Some(registry) = &self.registry else {
            return;
        };

        for keys in self.positions.values() {
            match self.routes(keys).as_slice() {
                [] => {}
                [(key, cfg)] => {
                    registry.register_route(&key.method, &key.pattern, cfg);
                    registry.apply_route_config(&key.method, &key.pattern, cfg);
                }
                routes => describe_position(registry, routes),
            }
        }
    }
}

/// How one document describes the routes at one position
#[derive(Debug)]
struct DocumentPlan<'a> {
    doc: &'a str,
    /// A route whose parameter names the position is templated with in this document
    template: &'a RouteKey,
    /// The routes this document describes there, under the template's names
    routes: Vec<(&'a RouteKey, &'a OpenApiRouteConfig)>,
}

/// Plans how each document describes the routes at one position.
///
/// A route is described in the documents it is placed in - see [`placement`] - except that
/// a catch-all is left out of every document a parameter route of its method is described in:
/// the two would be one operation there, and the parameter route is the one such a document
/// can describe faithfully. So a catch-all bound to `v1` beside a parameter route bound to
/// `admin` is described in `v1`, and moving the parameter route out of a document gives the
/// catch-all its place there back.
///
/// A document takes one templated path for one position - OpenAPI forbids two that differ in
/// their parameter names alone - so every route described there is described under one set of
/// names: the one most of them are written with, the smallest on a tie. A route written with
/// others has its path parameters renamed, which the wire does not notice, since a path
/// parameter is read by position - but they are not the names its handler reads.
fn plan_position<'a>(
    registry: &'a OpenApiRegistry,
    routes: &[(&'a RouteKey, &'a OpenApiRouteConfig)],
) -> Vec<DocumentPlan<'a>> {
    let placed: Vec<_> = routes
        .iter()
        .map(|&(key, cfg)| (key, cfg, placement(registry, cfg)))
        .collect();

    registry
        .specs()
        .iter()
        .filter_map(|spec| {
            let doc = spec.name.as_str();
            let here: Vec<_> = placed
                .iter()
                .filter(|(_, _, docs)| docs.contains(&doc))
                .map(|&(key, cfg, _)| (key, cfg))
                .collect();

            let routes: Vec<_> = here
                .iter()
                .copied()
                .filter(|(key, _)| {
                    !key.is_catch_all()
                        || !here
                            .iter()
                            .any(|(other, _)| other.method == key.method && !other.is_catch_all())
                })
                .collect();

            let template = spelled_by_most(&routes)?;
            Some(DocumentPlan {
                doc,
                template,
                routes,
            })
        })
        .collect()
}

/// A route written with the parameter names most of `routes` are written with, the smallest
/// names on a tie; `None` when there are no routes
fn spelled_by_most<'a>(routes: &[(&'a RouteKey, &OpenApiRouteConfig)]) -> Option<&'a RouteKey> {
    let mut spellings: BTreeMap<Vec<&str>, (usize, &RouteKey)> = BTreeMap::new();
    for &(key, _) in routes {
        spellings
            .entry(key.param_names())
            .and_modify(|(count, _)| *count += 1)
            .or_insert((1, key));
    }

    // Visited from the smallest names up, and the first of the most common is kept
    spellings
        .into_values()
        .min_by_key(|(count, _)| Reverse(*count))
        .map(|(_, key)| key)
}

/// Writes the operations of every route at one position from their configurations alone -
/// see [`plan_position`].
fn describe_position(registry: &OpenApiRegistry, routes: &[(&RouteKey, &OpenApiRouteConfig)]) {
    let Some((first, _)) = routes.first() else {
        return;
    };

    // Whatever was written at this position, under whichever names, goes: every route there
    // is written again below
    registry.remove_position(&first.pattern);

    for plan in plan_position(registry, routes) {
        for (key, cfg) in plan.routes {
            registry.describe_route_as(
                &key.method,
                &key.pattern,
                &plan.template.pattern,
                cfg,
                &[plan.doc],
            );
        }
    }
}

/// The documents a route of an operation group is described in: the ones it is bound to that
/// exist, or the first spec when it is bound to none that does.
///
/// A route is bound to documents only after it is mapped, and only ever gains them, so one
/// whose documents all fail to exist has been described in the first spec all along - which
/// is also where `OpenApiRegistry::rebind_route` leaves a route rebound to such documents.
fn placement<'a>(registry: &'a OpenApiRegistry, cfg: &'a OpenApiRouteConfig) -> Vec<&'a str> {
    let specs = registry.specs();
    let docs: Vec<&str> = registry
        .docs_for(cfg)
        .into_iter()
        .filter(|doc| specs.iter().any(|spec| spec.name == *doc))
        .collect();

    if !docs.is_empty() {
        return docs;
    }

    specs
        .first()
        .map(|spec| vec![spec.name.as_str()])
        .unwrap_or_default()
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
        paths_in(registry, "v1")
    }

    fn paths_in(registry: &OpenApiRegistry, doc: &str) -> Value {
        let doc = registry.document_by_name(doc).expect("document");
        serde_json::to_value(doc).expect("serialize openapi doc")["paths"].clone()
    }

    /// An `OpenApiState` with a registry for two specs, `v1` (the default) and `admin`.
    fn two_docs_state() -> (OpenApiState, OpenApiRegistry) {
        let config =
            OpenApiConfig::new().with_specs([OpenApiSpec::new("v1"), OpenApiSpec::new("admin")]);
        let registry = OpenApiRegistry::new(config.clone());

        let state = OpenApiState {
            registry: Some(registry.clone()),
            config: Some(config),
            ..Default::default()
        };

        (state, registry)
    }

    fn summary(paths: &Value, path: &str, method: &str) -> Value {
        paths[path][method]["summary"].clone()
    }

    /// A catch-all is left out only of the documents a parameter route at its position is
    /// described in
    #[test]
    fn it_leaves_out_a_catch_all_only_where_a_parameter_route_is_described() {
        let (mut state, registry) = two_docs_state();

        map(&mut state, Method::GET, "/files/{*path}", "the rest");
        state.update_route_config(&key(Method::GET, "/files/{*path}"), |cfg| {
            cfg.with_doc("v1")
        });
        map(&mut state, Method::GET, "/files/{id}", "one segment");
        state.update_route_config(&key(Method::GET, "/files/{id}"), |cfg| {
            cfg.with_doc("admin")
        });

        let v1 = paths_in(&registry, "v1");
        let admin = paths_in(&registry, "admin");

        assert_eq!(summary(&v1, "/files/{path}", "get"), "the rest");
        assert!(v1.get("/files/{id}").is_none());
        assert_eq!(summary(&admin, "/files/{id}", "get"), "one segment");
        assert!(admin.get("/files/{path}").is_none());
    }

    /// Moving the parameter route to another document gives the catch-all its place back
    #[test]
    fn it_describes_a_catch_all_again_once_the_parameter_route_moves_away() {
        let (mut state, registry) = two_docs_state();

        map(&mut state, Method::GET, "/files/{*path}", "the rest");
        map(&mut state, Method::GET, "/files/{path}", "one segment");
        assert_eq!(
            summary(&paths_in(&registry, "v1"), "/files/{path}", "get"),
            "one segment"
        );

        state.update_route_config(&key(Method::GET, "/files/{path}"), |cfg| {
            cfg.with_doc("admin")
        });

        assert_eq!(
            summary(&paths_in(&registry, "v1"), "/files/{path}", "get"),
            "the rest"
        );
        assert_eq!(
            summary(&paths_in(&registry, "admin"), "/files/{path}", "get"),
            "one segment"
        );
    }

    #[test]
    fn it_leaves_out_a_catch_all_from_the_documents_it_shares_with_a_parameter_route() {
        let (mut state, registry) = two_docs_state();

        map(&mut state, Method::GET, "/files/{*path}", "the rest");
        state.update_route_config(&key(Method::GET, "/files/{*path}"), |cfg| {
            cfg.with_docs(["v1", "admin"])
        });
        map(&mut state, Method::GET, "/files/{id}", "one segment");
        state.update_route_config(&key(Method::GET, "/files/{id}"), |cfg| {
            cfg.with_doc("admin")
        });

        let v1 = paths_in(&registry, "v1");
        let admin = paths_in(&registry, "admin");

        assert_eq!(summary(&v1, "/files/{path}", "get"), "the rest");
        assert_eq!(summary(&admin, "/files/{id}", "get"), "one segment");
        assert!(admin.get("/files/{path}").is_none());
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

    /// The router replaces a route mapped again under another spelling, and so does the
    /// document - in a group with a catch-all as well, where both configurations used to be
    /// written back in whatever order the map yielded them
    #[test]
    fn it_replaces_a_catch_all_mapped_again_under_another_spelling() {
        for _ in 0..32 {
            let (mut state, registry) = configured_state();

            map(
                &mut state,
                Method::GET,
                "/files/{*path:string}",
                "old handler",
            );
            map(&mut state, Method::GET, "/files/{id}/meta", "unrelated");
            map(&mut state, Method::GET, "/files/{*path}", "new handler");

            assert_eq!(
                summary(&paths(&registry), "/files/{path}", "get"),
                "new handler"
            );
            assert_eq!(state.route_configs.len(), 2);
        }
    }

    #[test]
    fn it_replaces_a_parameter_route_mapped_again_under_another_spelling() {
        let (mut state, registry) = configured_state();

        let typed = key(Method::GET, "/users/{id:integer}");
        state.on_route_mapped(typed.clone(), super::OpenApiRouteConfig::default());
        state.update_route_config(&typed, |cfg| {
            cfg.with_summary("old handler")
                .with_description("old description")
        });
        map(&mut state, Method::GET, "/users/{id}", "new handler");

        let operation = &paths(&registry)["/users/{id}"]["get"];
        assert_eq!(operation["summary"], "new handler");
        assert!(operation.get("description").is_none());
        assert_eq!(operation["parameters"][0]["schema"]["type"], "string");
        assert_eq!(state.route_configs.len(), 1);

        // A group closing over the spelling that was replaced reaches nothing
        state.update_route_config(&typed, |cfg| cfg.with_summary("group"));
        assert_eq!(
            paths(&registry)["/users/{id}"]["get"]["summary"],
            "new handler"
        );
    }

    /// A group applies its configuration to every route it mapped when it closes, the
    /// spelling that was replaced included
    #[test]
    fn it_closes_a_group_that_mapped_a_route_under_two_spellings() {
        let mut app = crate::App::new().with_open_api(|cfg| cfg);

        app.group("/files", |files| {
            files.open_api(|cfg| cfg.with_summary("files"));
            files.map_get("/{*path:string}", |path: String| async move { path });
            files.map_get("/{*path}", |path: String| async move { path });
        });

        let registry = app.openapi.registry.clone().expect("registry");
        assert_eq!(summary(&paths(&registry), "/files/{path}", "get"), "files");
        assert_eq!(app.openapi.route_configs.len(), 1);
    }

    /// A route bound only to documents that do not exist stays where an unbound route is
    /// described, as `rebind_route` leaves one - the catch-all beside it does not take its
    /// place there
    #[test]
    fn it_keeps_a_route_bound_only_to_missing_documents_in_the_first_spec() {
        let (mut state, registry) = two_docs_state();

        map(&mut state, Method::GET, "/files/{*path}", "the rest");
        map(&mut state, Method::GET, "/files/{path}", "one segment");
        state.update_route_config(&key(Method::GET, "/files/{path}"), |cfg| {
            cfg.with_doc("missing")
        });

        assert_eq!(
            summary(&paths_in(&registry, "v1"), "/files/{path}", "get"),
            "one segment"
        );
        assert!(paths_in(&registry, "admin").get("/files/{path}").is_none());

        let undescribed = state.undescribed_catch_alls();
        assert_eq!(undescribed.len(), 1);
        assert_eq!(undescribed[0].2, ["v1"]);
    }

    /// Routes bound to documents before OpenAPI is configured are replayed per document too
    #[test]
    fn it_replays_a_catch_all_into_the_documents_no_parameter_route_takes() {
        let mut state = OpenApiState::default();

        map(&mut state, Method::GET, "/files/{*path}", "the rest");
        state.update_route_config(&key(Method::GET, "/files/{*path}"), |cfg| {
            cfg.with_docs(["v1", "admin"])
        });
        map(&mut state, Method::GET, "/files/{path}", "one segment");
        state.update_route_config(&key(Method::GET, "/files/{path}"), |cfg| {
            cfg.with_doc("admin")
        });

        let config =
            OpenApiConfig::new().with_specs([OpenApiSpec::new("v1"), OpenApiSpec::new("admin")]);
        let registry = OpenApiRegistry::new(config.clone());
        state.registry = Some(registry.clone());
        state.config = Some(config);
        state.replay_all_routes_to_registry();

        assert_eq!(
            summary(&paths_in(&registry, "v1"), "/files/{path}", "get"),
            "the rest"
        );
        assert_eq!(
            summary(&paths_in(&registry, "admin"), "/files/{path}", "get"),
            "one segment"
        );
    }

    #[test]
    fn it_names_only_the_documents_a_catch_all_is_left_out_of() {
        let (mut state, _) = two_docs_state();

        map(&mut state, Method::GET, "/files/{*path}", "the rest");
        state.update_route_config(&key(Method::GET, "/files/{*path}"), |cfg| {
            cfg.with_docs(["v1", "admin"])
        });
        map(&mut state, Method::GET, "/files/{id}", "one segment");
        state.update_route_config(&key(Method::GET, "/files/{id}"), |cfg| {
            cfg.with_docs(["admin", "missing"])
        });

        let undescribed = state.undescribed_catch_alls();
        assert_eq!(undescribed.len(), 1);

        let (catch_all, by, docs) = &undescribed[0];
        assert_eq!(catch_all.pattern.as_ref(), "/files/{*path}");
        assert_eq!(by.pattern.as_ref(), "/files/{id}");
        assert_eq!(docs, &["admin"]);
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

        let (catch_all, by, docs) = &undescribed[0];
        assert_eq!(
            super::undescribed_catch_all_warning(catch_all, by, docs),
            "OpenAPI: `GET /files/{*path}` is left out of `v1`. OpenAPI describes a catch-all \
             as a one-segment path parameter, which `GET /files/{path}` already is in that \
             document, and one templated path cannot carry two operations for one method."
        );

        // Nothing is left out of a document that does not exist
        assert!(OpenApiState::default().undescribed_catch_alls().is_empty());
    }

    /// The path parameters `method` has at `path`, by name and schema type
    fn path_params(paths: &Value, path: &str, method: &str) -> Vec<(String, Value)> {
        paths[path][method]["parameters"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|parameter| parameter["in"] == "path")
            .map(|parameter| {
                let name = parameter["name"].as_str().expect("parameter name");
                (name.to_string(), parameter["schema"]["type"].clone())
            })
            .collect()
    }

    fn path_keys(paths: &Value) -> Vec<&str> {
        paths
            .as_object()
            .expect("paths")
            .keys()
            .map(String::as_str)
            .collect()
    }

    /// OpenAPI forbids two templated paths that differ in their parameter names alone, so the
    /// routes at one position are described under one set of names
    #[test]
    fn it_describes_the_routes_at_one_position_under_one_set_of_names() {
        let (mut state, registry) = configured_state();

        map(&mut state, Method::GET, "/users/{id}", "read");
        map(&mut state, Method::POST, "/users/{name}", "write");

        let paths = paths(&registry);
        assert_eq!(path_keys(&paths), ["/users/{id}"]);
        assert_eq!(summary(&paths, "/users/{id}", "get"), "read");
        assert_eq!(summary(&paths, "/users/{id}", "post"), "write");
        assert_eq!(
            path_params(&paths, "/users/{id}", "post"),
            [("id".to_string(), Value::from("string"))]
        );
    }

    #[test]
    fn it_takes_the_names_most_routes_at_a_position_are_written_with_in_any_order() {
        let routes = [
            (Method::GET, "/posts/{slug}"),
            (Method::PUT, "/posts/{slug}"),
            (Method::DELETE, "/posts/{id}"),
        ];

        for rotation in 0..routes.len() {
            let (mut state, registry) = configured_state();
            for (method, pattern) in routes.iter().cycle().skip(rotation).take(routes.len()) {
                map(&mut state, method.clone(), pattern, pattern);
            }

            let paths = paths(&registry);
            assert_eq!(path_keys(&paths), ["/posts/{slug}"]);
            assert_eq!(
                path_params(&paths, "/posts/{slug}", "delete"),
                [("slug".to_string(), Value::from("string"))]
            );
        }
    }

    #[test]
    fn it_describes_a_catch_all_under_the_names_of_its_position() {
        let (mut state, registry) = configured_state();

        map(&mut state, Method::GET, "/files/{*path}", "the rest");
        map(&mut state, Method::POST, "/files/{id}", "upload");

        let paths = paths(&registry);
        assert_eq!(path_keys(&paths), ["/files/{id}"]);
        assert_eq!(summary(&paths, "/files/{id}", "get"), "the rest");
        assert_eq!(
            path_params(&paths, "/files/{id}", "get"),
            [("id".to_string(), Value::from("string"))]
        );
    }

    /// A catch-all left out of a document is not described there, so its names count for
    /// nothing either
    #[test]
    fn it_names_a_position_after_the_routes_a_document_describes() {
        let (mut state, registry) = configured_state();

        map(&mut state, Method::GET, "/files/{*rest}", "the rest");
        map(&mut state, Method::GET, "/files/{a}", "one segment");
        map(&mut state, Method::POST, "/files/{rest}", "upload");

        let paths = paths(&registry);
        assert_eq!(path_keys(&paths), ["/files/{a}"]);
        assert_eq!(summary(&paths, "/files/{a}", "get"), "one segment");
        assert_eq!(summary(&paths, "/files/{a}", "post"), "upload");
    }

    #[test]
    fn it_names_a_position_in_each_document_after_the_routes_described_there() {
        let (mut state, registry) = two_docs_state();

        map(&mut state, Method::GET, "/users/{id}", "read");
        map(&mut state, Method::POST, "/users/{name}", "write");
        state.update_route_config(&key(Method::POST, "/users/{name}"), |cfg| {
            cfg.with_doc("admin")
        });

        // Each document describes one route there, under the names it is written with
        assert_eq!(path_keys(&paths_in(&registry, "v1")), ["/users/{id}"]);
        assert_eq!(path_keys(&paths_in(&registry, "admin")), ["/users/{name}"]);

        state.update_route_config(&key(Method::GET, "/users/{id}"), |cfg| {
            cfg.with_docs(["v1", "admin"])
        });

        let admin = paths_in(&registry, "admin");
        assert_eq!(path_keys(&admin), ["/users/{id}"]);
        assert_eq!(summary(&admin, "/users/{id}", "get"), "read");
        assert_eq!(summary(&admin, "/users/{id}", "post"), "write");
        assert_eq!(path_keys(&paths_in(&registry, "v1")), ["/users/{id}"]);
    }

    /// Each parameter is renamed after the position it is at, so two routes naming two
    /// positions the other way around swap their names, and keep their own types
    #[test]
    fn it_renames_path_parameters_by_position() {
        let (mut state, registry) = configured_state();

        map(&mut state, Method::GET, "/pairs/{a}/{b}", "read");
        state.on_route_mapped(
            key(Method::POST, "/pairs/{b:integer}/{a}"),
            super::OpenApiRouteConfig::default(),
        );

        let paths = paths(&registry);
        assert_eq!(path_keys(&paths), ["/pairs/{a}/{b}"]);
        assert_eq!(
            path_params(&paths, "/pairs/{a}/{b}", "post"),
            [
                ("a".to_string(), Value::from("integer")),
                ("b".to_string(), Value::from("string")),
            ]
        );
    }

    /// A `NamedPath<T>` describes its fields under the names its route is written with, and
    /// they are renamed with the route's own parameters rather than added beside them
    #[test]
    fn it_renames_the_path_parameters_a_named_path_describes() {
        #[derive(serde::Deserialize)]
        #[allow(dead_code)]
        struct ByName {
            name: u32,
        }

        let (mut state, registry) = configured_state();

        state.on_route_mapped(
            key(Method::POST, "/users/{name}"),
            super::OpenApiRouteConfig::default().consumes_named_path::<ByName>(),
        );
        map(&mut state, Method::GET, "/users/{id}", "read");

        assert_eq!(
            path_params(&paths(&registry), "/users/{id}", "post"),
            [("id".to_string(), Value::from("integer"))]
        );
    }

    #[test]
    fn it_describes_a_position_under_one_set_of_names_when_replaying_routes() {
        let mut state = OpenApiState::default();
        map(&mut state, Method::GET, "/users/{id}", "read");
        map(&mut state, Method::POST, "/users/{name}", "write");

        let config = OpenApiConfig::new().with_specs([OpenApiSpec::new("v1")]);
        let registry = OpenApiRegistry::new(config.clone());
        state.registry = Some(registry.clone());
        state.config = Some(config);
        state.replay_all_routes_to_registry();

        let paths = paths(&registry);
        assert_eq!(path_keys(&paths), ["/users/{id}"]);
        assert_eq!(summary(&paths, "/users/{id}", "post"), "write");
    }

    #[test]
    fn it_names_the_routes_it_describes_under_other_names() {
        let (mut state, _) = two_docs_state();

        map(&mut state, Method::GET, "/users/{id}", "read");
        map(&mut state, Method::POST, "/users/{name}", "write");
        state.update_route_config(&key(Method::POST, "/users/{name}"), |cfg| {
            cfg.with_docs(["v1", "admin"])
        });
        map(&mut state, Method::GET, "/pairs/{a}/{b}", "read");
        map(&mut state, Method::POST, "/pairs/{b}/{a}", "write");

        let warnings = state
            .renamed_routes()
            .into_iter()
            .map(|(route, template, docs)| super::renamed_route_warning(route, template, &docs))
            .collect::<Vec<_>>();

        // `admin` describes the write route alone, under its own names
        assert_eq!(
            warnings,
            [
                "OpenAPI: `POST /pairs/{b}/{a}` is described as `/pairs/{a}/{b}` in `v1`, with \
                 `b` as `a`, `a` as `b`. A templated path takes one set of parameter names, so \
                 the routes at one position are described under the names most of them are \
                 written with, the first in alphabetical order on a tie. Naming the parameters \
                 alike describes each route under its own.",
                "OpenAPI: `POST /users/{name}` is described as `/users/{id}` in `v1`, with \
                 `name` as `id`. A templated path takes one set of parameter names, so the \
                 routes at one position are described under the names most of them are written \
                 with, the first in alphabetical order on a tie. Naming the parameters alike \
                 describes each route under its own.",
            ]
        );

        // Nothing is renamed in a document that does not exist
        assert!(OpenApiState::default().renamed_routes().is_empty());
    }

    mod flattened {
        use serde::Deserialize;

        #[derive(Deserialize)]
        #[allow(dead_code)]
        pub(super) struct Inner {
            name: String,
        }

        #[derive(Deserialize)]
        #[allow(dead_code)]
        pub(super) struct Flat {
            #[serde(flatten)]
            inner: Inner,
            page: u32,
        }
    }

    use flattened::Flat;

    #[test]
    fn it_names_the_inputs_it_describes_without_their_fields() {
        let (mut state, _) = configured_state();

        let search = key(Method::GET, "/search");
        state.on_route_mapped(
            search.clone(),
            super::OpenApiRouteConfig::default().consumes_query::<Flat>(),
        );

        let items = key(Method::POST, "/items");
        state.on_route_mapped(
            items.clone(),
            super::OpenApiRouteConfig::default()
                .consumes_query::<Flat>()
                .consumes_json::<Flat>(),
        );

        let batch = key(Method::POST, "/batch");
        state.on_route_mapped(
            batch.clone(),
            super::OpenApiRouteConfig::default().consumes_json::<Vec<Flat>>(),
        );

        let warnings = state
            .undescribed_inputs()
            .into_iter()
            .map(|(route, input)| super::undescribed_input_warning(route, input))
            .collect::<Vec<_>>();

        // By route, then in the order the handler reads its inputs
        assert_eq!(
            warnings,
            [
                "OpenAPI: `POST /batch` describes its request body `Vec<Flat>` as any value: \
                 serde reads `struct Flat` inside it as a map, as it does a struct with a \
                 `#[serde(flatten)]` field, and a map does not name the keys it takes. \
                 Describe the body by hand with `.open_api(|c| c.with_request_schema(..))`.",
                "OpenAPI: `POST /items` describes none of the query parameters of `Flat`: \
                 serde reads `Flat` as a map, as it does a struct with a `#[serde(flatten)]` \
                 field, and a map does not name the keys it takes. Describe them by hand with \
                 `.open_api(|c| c.with_query_schema(..))`.",
                "OpenAPI: `POST /items` describes its request body `Flat` as an object without \
                 any fields: serde reads `Flat` as a map, as it does a struct with a \
                 `#[serde(flatten)]` field, and a map does not name the keys it takes. \
                 Describe the body by hand with `.open_api(|c| c.with_request_schema(..))`.",
                "OpenAPI: `GET /search` describes none of the query parameters of `Flat`: \
                 serde reads `Flat` as a map, as it does a struct with a `#[serde(flatten)]` \
                 field, and a map does not name the keys it takes. Describe them by hand with \
                 `.open_api(|c| c.with_query_schema(..))`.",
            ]
        );

        // Describing an input by hand is what the warning asks for, and ends it
        state.update_route_config(&search, |cfg| {
            cfg.with_query_schema(
                super::OpenApiSchema::object()
                    .with_property("page", super::OpenApiSchema::integer()),
            )
        });
        state.update_route_config(&items, |cfg| {
            cfg.with_request_schema(super::OpenApiSchema::object())
        });

        let left = state
            .undescribed_inputs()
            .into_iter()
            .map(|(route, input)| (route.pattern.as_ref(), input.kind()))
            .collect::<Vec<_>>();
        assert_eq!(
            left,
            [
                ("/batch", super::InputKind::RequestBody),
                ("/items", super::InputKind::QueryParameters),
            ]
        );
    }

    #[test]
    fn it_names_no_inputs_without_a_document_to_describe_them_in() {
        let mut state = OpenApiState::default();
        state.on_route_mapped(
            key(Method::POST, "/items"),
            super::OpenApiRouteConfig::default().consumes_json::<Flat>(),
        );

        assert!(state.undescribed_inputs().is_empty());
    }

    /// A catch-all left out of every document it is bound to describes nothing, its inputs
    /// included - it is reported as left out instead
    #[test]
    fn it_names_no_inputs_of_a_catch_all_left_out_of_every_document() {
        let (mut state, _) = configured_state();

        state.on_route_mapped(
            key(Method::GET, "/files/{*path}"),
            super::OpenApiRouteConfig::default().consumes_query::<Flat>(),
        );
        map(&mut state, Method::GET, "/files/{id}", "one segment");

        assert!(state.undescribed_inputs().is_empty());
        assert_eq!(state.undescribed_catch_alls().len(), 1);
    }

    /// A catch-all still described in one of its documents describes its inputs there
    #[test]
    fn it_names_the_inputs_of_a_catch_all_described_in_another_document() {
        let (mut state, _) = two_docs_state();

        let catch_all = key(Method::GET, "/files/{*path}");
        state.on_route_mapped(
            catch_all.clone(),
            super::OpenApiRouteConfig::default().consumes_query::<Flat>(),
        );
        state.update_route_config(&catch_all, |cfg| cfg.with_docs(["v1", "admin"]));
        map(&mut state, Method::GET, "/files/{id}", "one segment");

        let named = state
            .undescribed_inputs()
            .into_iter()
            .map(|(route, input)| (route.pattern.as_ref(), input.kind()))
            .collect::<Vec<_>>();
        assert_eq!(
            named,
            [("/files/{*path}", super::InputKind::QueryParameters)]
        );
    }

    #[test]
    fn it_spells_a_type_without_its_module_paths() {
        for (full, short) in [
            ("app::Flat", "Flat"),
            ("alloc::vec::Vec<app::models::Flat>", "Vec<Flat>"),
            (
                "core::option::Option<(u32, alloc::string::String)>",
                "Option<(u32, String)>",
            ),
            ("&str", "&str"),
            ("Flat", "Flat"),
        ] {
            assert_eq!(super::short_type_name(full), short);
        }
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
