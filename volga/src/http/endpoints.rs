//! Endpoints mapping utilities

use super::endpoints::{
    handlers::RouteHandler,
    route::{
        PathArgs, RouteEndpoint, RouteNode, RoutePipeline, is_catch_all_segment, join_path,
        split_path,
    },
};
use hyper::{Method, Uri};
use std::sync::Arc;

#[cfg(feature = "middleware")]
use {
    super::endpoints::route::Layer,
    crate::headers::{ACCESS_CONTROL_REQUEST_METHOD, HeaderMap, ORIGIN},
    crate::http::cors::CorsOverride,
    crate::middleware::MiddlewareFn,
};

pub mod args;
pub(crate) mod handlers;
pub(crate) mod meta;
pub(crate) mod route;

/// The catch-all a route group's fallback reads the rest of the path with, below the group's
/// prefix
const FALLBACK_TAIL: &str = "{*rest}";

/// Describes a mapping between HTTP Verbs, routes and request handlers
pub(crate) struct Endpoints {
    routes: RouteNode,
}

/// Specifies statuses that could be returned after route matching
pub(crate) enum FindResult {
    RouteNotFound,
    MethodNotFound(Arc<str>),
    Ok(Endpoint),
    /// No route answers the path, and the route group claiming its prefix answers with its
    /// own fallback, whatever the method
    Fallback(Endpoint),
}

/// Describes the endpoint that could be either a request handler
/// or a middleware pipeline with a request handler at the end.
pub(crate) struct Endpoint {
    /// Request handler or middleware pipeline
    pub(crate) pipeline: RoutePipeline,

    /// Current request path parameters with their values
    pub(crate) params: PathArgs,

    #[cfg(feature = "middleware")]
    pub(super) cors: CorsOverride,
}

impl Endpoint {
    /// Creates a new endpoint with the given request handler and path parameters
    #[inline]
    fn new(
        pipeline: RoutePipeline,
        params: PathArgs,
        #[cfg(feature = "middleware")] cors: CorsOverride,
    ) -> Self {
        Self {
            pipeline,
            params,
            #[cfg(feature = "middleware")]
            cors,
        }
    }

    /// Converts the endpoint into a tuple of (request handler, path parameters)
    #[inline]
    #[cfg(feature = "middleware")]
    pub(crate) fn into_parts(self) -> (RoutePipeline, PathArgs, CorsOverride) {
        (self.pipeline, self.params, self.cors)
    }

    #[inline]
    #[cfg(not(feature = "middleware"))]
    pub(crate) fn into_parts(self) -> (RoutePipeline, PathArgs) {
        (self.pipeline, self.params)
    }
}

impl Endpoints {
    /// Creates a new endpoints collection
    #[inline]
    pub(crate) fn new() -> Self {
        Self {
            routes: RouteNode::new(),
        }
    }

    /// Gets a context of the executing route by its `HttpRequest`
    #[inline]
    pub(crate) fn find(
        &self,
        method: &Method,
        uri: &Uri,
        #[cfg(feature = "middleware")] cors_enabled: bool,
        #[cfg(feature = "middleware")] headers: &HeaderMap,
    ) -> FindResult {
        let route_params = match self.routes.find(uri.path()) {
            Some(params) => params,
            None => return FindResult::RouteNotFound,
        };

        // Nothing is mapped here for any method, so this is no route - but a route group
        // claimed the position, and its fallback answers every method at it
        let Some(handlers) = route_params.route.endpoints() else {
            let Some(fallback) = route_params.route.fallback.as_deref() else {
                return FindResult::RouteNotFound;
            };

            // A catch-all is the last thing a lookup binds, so the router's own tail is the
            // last argument
            let mut params = route_params.params;
            if fallback.hides_tail {
                params.pop();
            }

            return FindResult::Fallback(Endpoint::new(
                fallback.pipeline.clone(),
                labelled(params, fallback.params.as_deref()),
                #[cfg(feature = "middleware")]
                fallback.cors.clone().unwrap_or_default(),
            ));
        };

        #[cfg(feature = "middleware")]
        if cors_enabled && method == Method::OPTIONS {
            // OPTIONS path: treat CORS preflight specially
            // (Only if it looks like a preflight request)
            let origin_present = headers.contains_key(ORIGIN);
            let acrm = headers
                .get(ACCESS_CONTROL_REQUEST_METHOD)
                .and_then(|v| Method::from_bytes(v.as_bytes()).ok());

            if origin_present && let Some(target_method) = acrm {
                // Check if the target method exists for this path
                return endpoint_for(handlers, &target_method).map_or_else(
                    || FindResult::MethodNotFound(route_params.route.allowed_methods()),
                    |handler| {
                        FindResult::Ok(Endpoint::new(
                            handler.pipeline.clone(),
                            labelled(route_params.params, handler.params.as_deref()),
                            #[cfg(feature = "middleware")]
                            handler.cors.clone().unwrap_or_default(),
                        ))
                    },
                );
            }
        }

        // Normal OPTIONS: keep existing behavior (likely 405 unless the user actually mapped OPTIONS)
        endpoint_for(handlers, method).map_or_else(
            || FindResult::MethodNotFound(route_params.route.allowed_methods()),
            |handler| {
                FindResult::Ok(Endpoint::new(
                    handler.pipeline.clone(),
                    labelled(route_params.params, handler.params.as_deref()),
                    #[cfg(feature = "middleware")]
                    handler.cors.clone().unwrap_or_default(),
                ))
            },
        )
    }

    /// Maps the request handler to the current HTTP Verb and route pattern
    #[inline]
    pub(crate) fn map_route(&mut self, method: Method, pattern: &str, handler: RouteHandler) {
        self.routes.insert(pattern, method, handler.into());
    }

    /// Maps the request layer to the current HTTP Verb and route pattern
    #[inline]
    #[cfg(feature = "middleware")]
    pub(crate) fn map_layer(&mut self, method: Method, pattern: &str, handler: Layer) {
        self.routes.insert(pattern, method, handler);
    }

    /// Maps a `GET` route the framework answers on the application's behalf, unless a `GET`
    /// route is mapped at `pattern` already
    ///
    /// The route is left out of the route listing, and a `GET` route mapped by hand at
    /// `pattern` later on takes its place.
    #[inline]
    #[cfg(feature = "static-files")]
    pub(crate) fn map_implicit_get(&mut self, pattern: &str, pipeline: RoutePipeline) {
        self.routes.insert_implicit(pattern, pipeline);
    }

    /// Maps a route group's fallback under `prefix`, answering every method where no route
    /// is mapped - see [`fallback_positions`] for where that is - and replacing the one
    /// mapped there already
    #[inline]
    pub(crate) fn map_fallback(&mut self, prefix: &str, handler: RouteHandler) {
        for (pattern, hides_tail) in fallback_positions(prefix) {
            self.routes
                .insert_fallback(&pattern, Arc::clone(&handler), hides_tail);
        }
    }

    /// Inserts a route group's middleware ahead of the layers the fallback under `prefix`
    /// already holds
    #[inline]
    #[cfg(feature = "middleware")]
    pub(crate) fn prepend_fallback_layers(&mut self, prefix: &str, layers: &[MiddlewareFn]) {
        for (pattern, _) in fallback_positions(prefix) {
            if let Some(fallback) = self
                .routes
                .find_mut(&pattern)
                .and_then(|route| route.fallback_mut())
            {
                fallback.prepend(layers);
            }
        }
    }

    /// Binds CORS headers to the fallback under `prefix`, unless something has already bound
    /// a policy of its own to it
    #[inline]
    #[cfg(feature = "middleware")]
    pub(crate) fn bind_fallback_cors_if_unset(&mut self, prefix: &str, cors: CorsOverride) {
        for (pattern, _) in fallback_positions(prefix) {
            if let Some(fallback) = self
                .routes
                .find_mut(&pattern)
                .and_then(|route| route.fallback_mut())
            {
                fallback.cors.get_or_insert_with(|| cors.clone());
            }
        }
    }

    /// Binds CORS headers to the route handler
    #[inline]
    #[cfg(feature = "middleware")]
    pub(crate) fn bind_cors(&mut self, method: &Method, pattern: &str, cors: CorsOverride) {
        self.routes
            .find_mut(pattern)
            .map(|route| route.handler_mut(method).map(|h| h.cors = Some(cors)));
    }

    /// Binds CORS headers to the route handler, unless something has already bound a
    /// policy of its own to it
    #[inline]
    #[cfg(feature = "middleware")]
    pub(crate) fn bind_cors_if_unset(
        &mut self,
        method: &Method,
        pattern: &str,
        cors: CorsOverride,
    ) {
        self.routes.find_mut(pattern).map(|route| {
            route
                .handler_mut(method)
                .map(|h| h.cors.get_or_insert(cors))
        });
    }

    /// Inserts a route group's middleware ahead of the layers the route already holds
    #[inline]
    #[cfg(feature = "middleware")]
    pub(crate) fn prepend_layers(
        &mut self,
        method: &Method,
        pattern: &str,
        layers: &[MiddlewareFn],
    ) {
        self.routes
            .find_mut(pattern)
            .map(|route| route.handler_mut(method).map(|h| h.prepend(layers)));
    }

    /// Returns `true` if `pattern` is mapped for `method`
    #[inline]
    #[cfg_attr(not(test), allow(dead_code))]
    pub(crate) fn contains(&mut self, method: &Method, pattern: &str) -> bool {
        self.routes
            .find(pattern)
            .map(|params| {
                params
                    .route
                    .handlers
                    .as_ref()
                    .is_some_and(|h| h.binary_search_by(|r| r.cmp(method)).is_ok())
            })
            .unwrap_or(false)
    }

    /// Traverses the route tree and collects all available routes.
    /// Returns a vector of tuples containing (HTTP method, route path)
    pub(crate) fn collect(&self) -> meta::RoutesInfo {
        self.routes.collect()
    }

    #[cfg(feature = "middleware")]
    pub(crate) fn compose(&mut self) {
        self.routes.compose();
    }
}

/// The positions a route group's fallback answers at under `prefix`, each with whether the
/// router reads its tail there with a catch-all of its own
///
/// That is the prefix itself and `{*rest}` below it, since a catch-all reads at least one
/// segment. A prefix ending in a catch-all of its own - `/files/{*path}` - is the one
/// position: that catch-all reads everything below the prefix already, and nothing can
/// follow one.
fn fallback_positions(prefix: &str) -> impl Iterator<Item = (String, bool)> {
    let tail = !split_path(prefix).last().is_some_and(is_catch_all_segment);

    std::iter::once((prefix.to_owned(), false))
        .chain(tail.then(|| (join_path(prefix, FALLBACK_TAIL), true)))
}

/// Labels the matched path arguments with `names`, the parameter names of the endpoint
/// answering
///
/// The tree binds them under the names of whichever route reached each position first, and
/// an endpoint carries names of its own only when it was written with different ones - so
/// this is a branch and nothing else for every route that agrees with the tree.
#[inline]
fn labelled(mut params: PathArgs, names: Option<&[Arc<str>]>) -> PathArgs {
    if let Some(names) = names {
        params.rename(names);
    }
    params
}

/// Picks the endpoint that answers `method`
///
/// A `GET` route answers a `HEAD` request that has no route of its own: `HEAD` is `GET`
/// without content (RFC 9110 Section 9.3.2), so the request travels through everything
/// that route travels through, and the body is dropped on the way out. A `HEAD` mapped by
/// hand is found here first and keeps the `GET` route out of it.
#[inline]
fn endpoint_for<'route>(
    handlers: &'route [RouteEndpoint],
    method: &Method,
) -> Option<&'route RouteEndpoint> {
    match handlers.binary_search_by(|handler| handler.cmp(method)) {
        Ok(i) => Some(&handlers[i]),
        Err(_) if method == Method::HEAD => handlers
            .binary_search_by(|handler| handler.cmp(&Method::GET))
            .ok()
            .map(|i| &handlers[i]),
        Err(_) => None,
    }
}

#[cfg(test)]
mod tests {
    use super::{Endpoints, FindResult, handlers::Func};
    #[cfg(feature = "middleware")]
    use crate::headers::HeaderMap;
    use crate::ok;
    use hyper::{Method, Request};

    #[test]
    fn it_maps_and_gets_endpoint() {
        let mut endpoints = Endpoints::new();

        let handler = Func::new(|| async { ok!() });

        endpoints.map_route(Method::POST, "path/to/handler", handler);

        let request = Request::post("https://example.com/path/to/handler")
            .body(())
            .unwrap();
        let post_handler = endpoints.find(
            request.method(),
            request.uri(),
            #[cfg(feature = "middleware")]
            false,
            #[cfg(feature = "middleware")]
            &HeaderMap::new(),
        );

        match post_handler {
            FindResult::Ok(_) => (),
            _ => panic!("`post_handler` must be is the `Ok` state"),
        }
    }

    #[test]
    fn it_returns_route_not_found() {
        let mut endpoints = Endpoints::new();

        let handler = Func::new(|| async { ok!() });

        endpoints.map_route(Method::POST, "path/to/handler", handler);

        let request = Request::post("https://example.com/path/to/another-handler")
            .body(())
            .unwrap();
        let post_handler = endpoints.find(
            request.method(),
            request.uri(),
            #[cfg(feature = "middleware")]
            false,
            #[cfg(feature = "middleware")]
            &HeaderMap::new(),
        );

        match post_handler {
            FindResult::RouteNotFound => (),
            _ => panic!("`post_handler` must be is the `RouteNotFound` state"),
        }
    }

    #[test]
    fn it_returns_method_not_found() {
        let mut endpoints = Endpoints::new();

        let handler = Func::new(|| async { ok!() });

        endpoints.map_route(Method::GET, "path/to/handler", handler);

        let request = Request::post("https://example.com/path/to/handler")
            .body(())
            .unwrap();
        let post_handler = endpoints.find(
            request.method(),
            request.uri(),
            #[cfg(feature = "middleware")]
            false,
            #[cfg(feature = "middleware")]
            &HeaderMap::new(),
        );

        match post_handler {
            // HEAD is answered by the GET route, so the resource supports it
            FindResult::MethodNotFound(allow) => assert_eq!(allow.as_ref(), "GET,HEAD"),
            _ => panic!("`post_handler` must be is the `MethodNotFound` state"),
        }
    }

    #[test]
    fn is_has_route_after_map() {
        let mut endpoints = Endpoints::new();

        let handler = Func::new(|| async { ok!() });

        endpoints.map_route(Method::GET, "path/to/handler", handler);

        let has_route = endpoints.contains(&Method::GET, "path/to/handler");

        assert!(has_route);
    }

    /// The tree binds a path argument under the name of whichever route reached its
    /// position first; the endpoint that answers decides what the request is labelled with
    #[test]
    fn it_labels_path_args_with_the_names_of_the_endpoint_that_answers() {
        let mut endpoints = Endpoints::new();

        let handler = Func::new(|| async { ok!() });

        endpoints.map_route(Method::GET, "/users/{id}", handler.clone());
        endpoints.map_route(Method::POST, "/users/{name}", handler);

        for (method, expected) in [(Method::GET, "id"), (Method::POST, "name")] {
            let request = Request::builder()
                .method(method.clone())
                .uri("https://example.com/users/42")
                .body(())
                .unwrap();

            let found = endpoints.find(
                request.method(),
                request.uri(),
                #[cfg(feature = "middleware")]
                false,
                #[cfg(feature = "middleware")]
                &HeaderMap::new(),
            );

            match found {
                FindResult::Ok(endpoint) => {
                    let arg = endpoint.params.first().expect("the route has a parameter");

                    assert_eq!(arg.name.as_ref(), expected, "{method}");
                    assert_eq!(arg.value.as_ref(), "42", "{method}");
                }
                _ => panic!("{method} must have matched"),
            }
        }
    }

    /// Looks `method` on `path` up in `endpoints`
    fn find(endpoints: &Endpoints, method: Method, path: &str) -> FindResult {
        let request = Request::builder()
            .method(method)
            .uri(format!("https://example.com{path}"))
            .body(())
            .unwrap();

        endpoints.find(
            request.method(),
            request.uri(),
            #[cfg(feature = "middleware")]
            false,
            #[cfg(feature = "middleware")]
            &HeaderMap::new(),
        )
    }

    #[test]
    fn it_finds_the_fallback_where_no_route_is_mapped() {
        let mut endpoints = Endpoints::new();

        endpoints.map_route(Method::GET, "/api/models", Func::new(|| async { ok!() }));
        endpoints.map_fallback("/api", Func::new(|| async { ok!() }));

        for method in [Method::GET, Method::DELETE, Method::OPTIONS] {
            for path in ["/api", "/api/nope"] {
                assert!(
                    matches!(
                        find(&endpoints, method.clone(), path),
                        FindResult::Fallback(_)
                    ),
                    "{method} {path}"
                );
            }
        }

        // A route answers its own position, for the methods it has and with a 405 for the
        // rest
        assert!(matches!(
            find(&endpoints, Method::GET, "/api/models"),
            FindResult::Ok(_)
        ));
        match find(&endpoints, Method::POST, "/api/models") {
            FindResult::MethodNotFound(allow) => assert_eq!(allow.as_ref(), "GET,HEAD"),
            _ => panic!("expected the route's 405"),
        }

        assert!(matches!(
            find(&endpoints, Method::GET, "/other"),
            FindResult::RouteNotFound
        ));
    }

    /// A route mapped at the fallback's tail takes that position over, for every method
    #[test]
    fn it_leaves_the_tail_to_a_catch_all_route_mapped_there() {
        let mut endpoints = Endpoints::new();

        endpoints.map_route(Method::GET, "/api/{*path}", Func::new(|| async { ok!() }));
        endpoints.map_fallback("/api", Func::new(|| async { ok!() }));

        match find(&endpoints, Method::GET, "/api/a/b") {
            FindResult::Ok(endpoint) => {
                assert_eq!(endpoint.params.first().unwrap().name.as_ref(), "path")
            }
            _ => panic!("the route answers its own method"),
        }

        assert!(matches!(
            find(&endpoints, Method::PUT, "/api/a/b"),
            FindResult::MethodNotFound(_)
        ));
    }

    /// Collects the `(name, value)` pairs a fallback answering `path` is handed
    fn fallback_params(endpoints: &Endpoints, path: &str) -> Vec<(String, String)> {
        match find(endpoints, Method::DELETE, path) {
            FindResult::Fallback(endpoint) => endpoint
                .params
                .iter()
                .map(|arg| (arg.name.to_string(), arg.value.to_string()))
                .collect(),
            _ => panic!("the fallback answers {path}"),
        }
    }

    /// The tail below a group's prefix is read by a catch-all the application never wrote,
    /// so the fallback is handed the parameters its prefix declares - the same at the prefix
    /// and below it, under the names the prefix gives them
    #[test]
    fn it_hands_a_fallback_the_parameters_of_its_prefix_alone() {
        let mut endpoints = Endpoints::new();

        // Another route reaches the position first and names it differently
        endpoints.map_route(Method::GET, "/t/{org}/users", Func::new(|| async { ok!() }));
        endpoints.map_fallback("/t/{tenant}", Func::new(|| async { ok!() }));
        endpoints.map_fallback("/x/{rest}", Func::new(|| async { ok!() }));

        let tenant = vec![("tenant".to_string(), "acme".to_string())];
        for path in ["/t/acme", "/t/acme/nope", "/t/acme/a/b"] {
            assert_eq!(fallback_params(&endpoints, path), tenant, "{path}");
        }

        // A parameter of the prefix named like the tail is not doubled by it
        let rest = vec![("rest".to_string(), "acme".to_string())];
        for path in ["/x/acme", "/x/acme/nope"] {
            assert_eq!(fallback_params(&endpoints, path), rest, "{path}");
        }
    }

    /// A prefix ending in a catch-all is the fallback's one position, and that catch-all is
    /// the application's own, so it is bound
    #[test]
    fn it_binds_the_catch_all_a_prefix_ends_in() {
        let mut endpoints = Endpoints::new();
        endpoints.map_fallback("/files/{*path}", Func::new(|| async { ok!() }));

        assert_eq!(
            fallback_params(&endpoints, "/files/a/b"),
            vec![("path".to_string(), "a/b".to_string())]
        );
        assert!(matches!(
            find(&endpoints, Method::GET, "/files"),
            FindResult::RouteNotFound
        ));
    }

    /// A preflight looks for the endpoint of the method it asks about, and a fallback is
    /// not one: it answers the preflight request itself, as it answers every method
    #[test]
    #[cfg(feature = "middleware")]
    fn it_finds_the_fallback_for_a_preflight() {
        use crate::headers::{ACCESS_CONTROL_REQUEST_METHOD, HeaderValue, ORIGIN};

        let mut endpoints = Endpoints::new();
        endpoints.map_fallback("/api", Func::new(|| async { ok!() }));

        let mut headers = HeaderMap::new();
        headers.insert(ORIGIN, HeaderValue::from_static("https://example.test"));
        headers.insert(
            ACCESS_CONTROL_REQUEST_METHOD,
            HeaderValue::from_static("GET"),
        );

        let request = Request::options("https://example.com/api/nope")
            .body(())
            .unwrap();
        let found = endpoints.find(request.method(), request.uri(), true, &headers);

        assert!(matches!(found, FindResult::Fallback(_)));
    }

    #[test]
    fn is_doesnt_have_route_after_map_a_different_one() {
        let mut endpoints = Endpoints::new();

        let handler = Func::new(|| async { ok!() });

        endpoints.map_route(Method::GET, "path/to/handler", handler);

        let has_route = endpoints.contains(&Method::PUT, "path/to/handler");

        assert!(!has_route);
    }
}
