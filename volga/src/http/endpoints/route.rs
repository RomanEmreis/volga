//! # Route Tree Implementation
//!
//! This module implements a hierarchical route tree optimized for fast lookups
//! and minimal runtime overhead. Instead of using a `HashMap` for storing child
//! routes or handlers, this implementation relies on sorted `Vec`s combined with
//! binary search. This design choice is intentional and based on the following
//! observations:
//!
//! - **Read-heavy, write-once workload:**  
//!   The route tree is fully constructed during application startup and remains
//!   immutable during request handling. As a result, hash table insert overhead,
//!   rehashing, and memory fragmentation provide no advantage compared to a
//!   contiguous `Vec` structure.
//!
//! - **Better cache locality:**  
//!   Route lookup involves traversing a small number of nodes and comparing short
//!   path segments. Using a compact `Vec` means that route entries are stored
//!   contiguously in memory, which improves CPU cache hit rates and branch
//!   prediction compared to the pointer-heavy layout of a `HashMap`.
//!
//! - **Predictable binary search cost:**  
//!   Each route level uses a sorted `Vec` of static segments and performs a
//!   `binary_search_by`. The number of elements per level is typically small
//!   (dozens at most), making binary search faster in practice than hash lookup
//!   due to lower constant factors and better branch predictability.
//!
//! - **Dynamic routes are rare and handled separately:**  
//!   Each node may have at most one dynamic child (e.g., `/user/{id}`), stored
//!   as an `Option<RouteEntry>`. This avoids unnecessary branching and memory
//!   overhead in the common case of static routing. A parameter is matched by the
//!   position it sits at rather than by what it is called, so the child is shared by
//!   every route running through it - but each endpoint remembers the name its own
//!   pattern was written with, so `GET /users/{id}` and `POST /users/{name}` are two
//!   routes that bind two names at one position.
//!
//! ## Use of `SmallVec`
//!
//! `SmallVec` is used for short collections such as `PathArgs`, which typically
//! contain zero or one path parameters. `SmallVec<[T; N]>` stores elements
//! directly on the stack for small `N`, avoiding heap allocations in the common
//! case. Since these values are later moved into heap-allocated request
//! extensions, this approach eliminates an early allocation while preserving
//! performance for longer paths.
//!
//! In summary, this design prioritizes **low per-request latency, cache
//! efficiency, and predictable memory access patterns** over theoretical
//! O(1) lookup complexity, which in practice provides better real-world
//! performance under concurrent, read-only workloads.

use crate::utils::str::memchr_split_nonempty;
use hyper::Method;
use smallvec::SmallVec;
use std::sync::Arc;

#[cfg(feature = "middleware")]
use {crate::http::cors::CorsOverride, crate::middleware::MiddlewareFn};

pub(crate) use layer::{Layer, RoutePipeline};
pub(crate) use path_args::{PathArg, PathArgs};

pub(crate) mod layer;
pub(crate) mod path_args;

const OPEN_BRACKET: char = '{';
const CLOSE_BRACKET: char = '}';
const PATH_SEPARATOR: u8 = b'/';
const DOUBLE_PATH_SEPARATOR: &str = "//";
const ROOT_PATH: &str = "/";
const TYPE_SEPARATOR: char = ':';
const ALLOW_METHOD_SEPARATOR: char = ',';
const DEFAULT_DEPTH: usize = 4;

/// The route parameter names of one pattern, in the order the pattern writes them
pub(super) type ParamNames = SmallVec<[Arc<str>; DEFAULT_DEPTH]>;

/// Represents a full route's "local" middleware pipeline
/// with handler
#[derive(Clone)]
pub(super) struct RouteEndpoint {
    pub(super) method: Method,
    pub(super) pipeline: RoutePipeline,
    /// The parameter names this endpoint's own pattern was written with, kept only when
    /// they differ from the ones the tree binds on the way here - which happens when
    /// another verb reached this position first and named it something else. `None` is
    /// the common case and costs a request nothing
    pub(super) params: Option<ParamNames>,
    /// The CORS policy bound to this route, `None` while nothing has bound one
    #[cfg(feature = "middleware")]
    pub(super) cors: Option<CorsOverride>,
}

/// Represents route path node
#[derive(Clone)]
pub(super) struct RouteEntry {
    path: Arc<str>,
    node: Box<RouteNode>,
}

/// A node in the route tree
#[derive(Clone)]
pub(super) struct RouteNode {
    /// A list of associated endpoints for each HTTP method
    pub(super) handlers: Option<SmallVec<[RouteEndpoint; DEFAULT_DEPTH]>>,

    /// List of static routes
    static_routes: SmallVec<[RouteEntry; DEFAULT_DEPTH]>,

    /// Dynamic route
    dynamic_route: Option<RouteEntry>,

    /// Cached allowed methods header value
    allowed_methods: Option<Arc<str>>,
}

/// Parameters of a route
pub(super) struct RouteParams<'route> {
    pub(super) route: &'route RouteNode,
    pub(super) params: PathArgs,
}

impl RouteEntry {
    /// Creates a new [`RouteEntry`]
    #[inline]
    fn new(path: &str) -> Self {
        Self {
            node: Box::new(RouteNode::new()),
            path: Arc::from(path),
        }
    }

    /// Compares two route entries
    #[inline(always)]
    fn cmp(&self, path: &str) -> std::cmp::Ordering {
        self.path.as_ref().cmp(path)
    }
}

impl RouteEndpoint {
    /// Creates a new [`RouteEndpoint`]
    #[inline]
    fn new(method: Method, params: Option<ParamNames>) -> Self {
        Self {
            method,
            pipeline: RoutePipeline::new(),
            params,
            #[cfg(feature = "middleware")]
            cors: None,
        }
    }

    /// The parameter names this endpoint's pattern was written with
    ///
    /// `bound` is what the tree binds on the way here, which is what the endpoint was
    /// written with unless it says otherwise.
    #[inline]
    fn params<'names>(&'names self, bound: &'names ParamNames) -> &'names [Arc<str>] {
        self.params.as_deref().unwrap_or(bound)
    }

    /// Inserts a layer into the pipeline
    #[inline]
    fn insert(&mut self, handler: Layer) {
        self.pipeline.insert(handler);
    }

    /// Inserts middleware ahead of the layers this endpoint already holds
    #[inline]
    #[cfg(feature = "middleware")]
    pub(super) fn prepend(&mut self, layers: &[MiddlewareFn]) {
        self.pipeline.prepend(layers);
    }

    /// Compares two route endpoints
    #[inline(always)]
    pub(super) fn cmp(&self, method: &Method) -> std::cmp::Ordering {
        let left = method_order(&self.method);
        let right = method_order(method);
        left.cmp(&right)
    }
}

impl RouteNode {
    /// Create a new [`RouteNode`]
    #[inline]
    pub(super) fn new() -> Self {
        Self {
            static_routes: SmallVec::new(),
            handlers: None,
            dynamic_route: None,
            allowed_methods: None,
        }
    }

    /// Inserts a handler to the route tree
    ///
    /// # Panics
    /// if this route is a second name for one already mapped for `method`, or for the
    /// `GET` that a `HEAD` answers. See [`ambiguous_route`].
    pub(super) fn insert(&mut self, path: &str, method: Method, handler: Layer) {
        let mut current = self;

        // What this pattern calls its parameters, and what the tree binds at the positions
        // they sit at - the same names, unless another route reached a position first
        let mut written = ParamNames::new();
        let mut bound = ParamNames::new();

        for segment in split_path(path) {
            if is_dynamic_segment(segment) {
                let name = Self::dynamic_name(segment);
                let (next, name_bound) = current.insert_dynamic_node(name);

                written.push(if name_bound.as_ref() == name {
                    Arc::clone(&name_bound)
                } else {
                    Arc::from(name)
                });

                bound.push(name_bound);
                current = next;
            } else {
                current = current.insert_static_node(segment);
            }
        }

        current.insert_handler(method, handler, &written, &bound, path);
    }

    /// Finds handlers by path
    #[inline]
    pub(super) fn find(&self, path: &str) -> Option<RouteParams<'_>> {
        let mut current = self;
        let mut params = PathArgs::new();
        let path_segments = split_path(path);

        for segment in path_segments {
            if let Ok(i) = current.static_routes.binary_search_by(|r| r.cmp(segment)) {
                current = current.static_routes[i].node.as_ref();
                continue;
            }

            if let Some(next) = &current.dynamic_route {
                params.push(PathArg {
                    name: Arc::clone(&next.path),
                    value: Box::from(segment),
                });
                current = next.node.as_ref();
                continue;
            }

            return None;
        }

        (!current.handlers.as_ref().is_none_or(|h| h.is_empty())).then_some(RouteParams {
            route: current,
            params,
        })
    }

    /// Finds handlers by path and returns a mutable reference to it
    #[inline]
    #[cfg(feature = "middleware")]
    pub(super) fn find_mut(&mut self, path: &str) -> Option<&'_ mut RouteNode> {
        let mut current = self;
        let path_segments = split_path(path);

        for segment in path_segments {
            if let Ok(i) = current.static_routes.binary_search_by(|r| r.cmp(segment)) {
                current = current.static_routes[i].node.as_mut();
                continue;
            }

            if let Some(next) = &mut current.dynamic_route {
                current = next.node.as_mut();
                continue;
            }

            return None;
        }

        (!current.handlers.as_ref().is_none_or(|h| h.is_empty())).then_some(current)
    }

    /// Returns a reference to the handler for the given method
    #[inline]
    #[allow(unused)]
    pub(super) fn handler(&self, method: &Method) -> Option<&RouteEndpoint> {
        let handlers = self.handlers.as_ref()?;
        let i = handlers.binary_search_by(|h| h.cmp(method)).ok()?;
        Some(&handlers[i])
    }

    /// Returns a mutable reference to the handler for the given method
    #[inline]
    #[cfg(feature = "middleware")]
    pub(super) fn handler_mut(&mut self, method: &Method) -> Option<&mut RouteEndpoint> {
        let i = self
            .handlers
            .as_ref()?
            .binary_search_by(|h| h.cmp(method))
            .ok()?;

        Some(&mut self.handlers.as_mut()?[i])
    }

    #[cfg(feature = "middleware")]
    pub(super) fn compose(&mut self) {
        // Compose all static routes
        self.static_routes.iter_mut().for_each(|r| r.node.compose());

        // Compose a dynamic route if present
        if let Some(route) = self.dynamic_route.as_mut() {
            route.node.compose();
        }

        // Compose oute endpoint pipeline if present
        if let Some(handlers) = self.handlers.as_mut() {
            handlers.iter_mut().for_each(|r| r.pipeline.compose());
        }
    }

    /// Returns allowed HTTP methods for this route
    #[inline]
    pub(super) fn allowed_methods(&self) -> Arc<str> {
        self.allowed_methods
            .as_ref()
            .map(Arc::clone)
            .unwrap_or_else(|| Arc::from(""))
    }

    /// Traverses the route tree and collects all available routes
    /// Returns a vector of tuples containing (HTTP method, route path)
    pub(super) fn collect(&self) -> super::meta::RoutesInfo {
        let mut routes = Vec::new();
        let mut segments = Vec::new();
        self.traverse_routes(&mut routes, &mut segments);
        super::meta::RoutesInfo(routes)
    }

    fn traverse_routes<'tree>(
        &'tree self,
        routes: &mut Vec<super::meta::RouteInfo>,
        segments: &mut Vec<PathSegment<'tree>>,
    ) {
        // Traverse static routes
        for route in self.static_routes.iter() {
            segments.push(PathSegment::Static(&route.path));
            route.node.traverse_routes(routes, segments);
            segments.pop();
        }

        // Traverse dynamic route (if any)
        if let Some(route) = &self.dynamic_route {
            segments.push(PathSegment::Dynamic(&route.path));
            route.node.traverse_routes(routes, segments);
            segments.pop();
        }

        // Record handlers for this node
        let Some(ref handlers) = self.handlers else {
            return;
        };

        for handler in handlers.iter() {
            // A route is listed the way it was written, which is the name the tree binds
            // unless another verb reached one of these positions first
            let route_path = spell_route(segments, handler.params.as_deref());
            routes.push(super::meta::RouteInfo::new(
                handler.method.clone(),
                &route_path,
            ));
        }
    }

    #[inline(always)]
    fn insert_static_node(&mut self, segment: &str) -> &mut Self {
        match self.static_routes.binary_search_by(|r| r.cmp(segment)) {
            Ok(i) => &mut self.static_routes[i].node,
            Err(i) => {
                self.static_routes.insert(i, RouteEntry::new(segment));
                &mut self.static_routes[i].node
            }
        }
    }

    /// Descends into the node the parameter named `name` leads to, creating it when this
    /// is the first route to name one at this position, and hands back the name bound
    /// there - `name` itself, unless another route got here first and called it something
    /// else.
    #[inline(always)]
    fn insert_dynamic_node(&mut self, name: &str) -> (&mut Self, Arc<str>) {
        let entry = self
            .dynamic_route
            .get_or_insert_with(|| RouteEntry::new(name));
        let bound = Arc::clone(&entry.path);

        (entry.node.as_mut(), bound)
    }

    #[inline(always)]
    fn insert_handler(
        &mut self,
        method: Method,
        handler: Layer,
        written: &ParamNames,
        bound: &ParamNames,
        path: &str,
    ) {
        if let Some(handlers) = self.handlers.as_ref()
            && let Some((other_method, other_written)) =
                conflicting_endpoint(handlers, &method, written, bound)
        {
            ambiguous_route(path, &method, written, &other_method, &other_written);
        }

        // A pattern naming its parameters the way the tree already binds them - the route
        // that reached each position first, and every route agreeing with it - says
        // nothing, and a request to it is labelled straight from the tree
        let params = (written != bound).then(|| written.clone());
        let handlers = self.handlers.get_or_insert_with(SmallVec::new);

        let endpoint = match handlers.binary_search_by(|r| r.cmp(&method)) {
            Ok(i) => {
                // Mapping a handler where one is already mapped replaces the route, and
                // takes with it every layer bound to the registration being replaced: a
                // handler and its middleware are written together, and answering with one
                // while running the other's middleware is a pipeline nobody wrote.
                // Layers added to a route do not replace it - only another handler does
                if matches!(handler, Layer::Handler(_)) {
                    handlers[i] = RouteEndpoint::new(method, params);
                }

                &mut handlers[i]
            }
            Err(i) => {
                handlers.insert(i, RouteEndpoint::new(method, params));
                &mut handlers[i]
            }
        };
        endpoint.insert(handler);
        self.allowed_methods = Some(make_allowed_str(handlers));
    }

    #[inline(always)]
    fn dynamic_name(segment: &str) -> &str {
        // expects "{name}" but safely handles unexpected input
        // only touch placeholders: "{id:integer}"
        if let Some(inner) = segment
            .strip_prefix(OPEN_BRACKET)
            .and_then(|s| s.strip_suffix(CLOSE_BRACKET))
        {
            // strip ":type" part from inside, keep braces in routing logic if you need,
            // but for *param name* return only name.
            let (name, _) = inner.split_once(TYPE_SEPARATOR).unwrap_or((inner, ""));
            // IMPORTANT: return name only (caller decides how to store)
            name
        } else {
            segment
        }
    }
}

/// Finds the endpoint at this node whose parameter names `method` has to agree with
///
/// A parameter is matched by the position it sits at rather than by what it is called, so
/// every route running through a position shares it - but each endpoint labels its request
/// with the names its own pattern was written with, so two verbs may call one position two
/// things and both be right. Two cases cannot:
///
/// - the same verb at the same node: the second registration replaces the first, and a
///   different name says that is not what was meant. `map_get("/users/{id}")` followed by
///   `map_get("/users/{name}")` leaves one route mapped, not two
/// - `GET` and `HEAD`: a `HEAD` request with no route of its own is answered by the `GET`
///   route (RFC 9110 Section 9.3.2), so the two describe one resource and cannot disagree
///   about what identifies it
///
/// `bound` is what the tree binds on the way to this node, which is what an endpoint was
/// written with unless it says otherwise.
#[inline]
fn conflicting_endpoint(
    handlers: &[RouteEndpoint],
    method: &Method,
    written: &ParamNames,
    bound: &ParamNames,
) -> Option<(Method, ParamNames)> {
    handlers
        .iter()
        .find(|endpoint| {
            (endpoint.method == *method || answers_for(&endpoint.method, method))
                && endpoint.params(bound) != written.as_slice()
        })
        .map(|endpoint| {
            (
                endpoint.method.clone(),
                endpoint.params(bound).iter().cloned().collect(),
            )
        })
}

/// Returns `true` when one of the two methods answers the requests of the other
#[inline(always)]
fn answers_for(left: &Method, right: &Method) -> bool {
    (*left == Method::GET && *right == Method::HEAD)
        || (*left == Method::HEAD && *right == Method::GET)
}

/// Reports a route written as a second name for one already mapped
///
/// Only one of the two names can label the request that arrives - the position they share
/// carries the route, and the endpoint answering it carries the name - so the route mapped
/// second used to take the first one's place while binding the first one's parameter name,
/// which is a route nobody wrote. There is nothing to pick between them, so this is
/// reported where it is written rather than resolved (#226).
#[cold]
#[inline(never)]
fn ambiguous_route(
    path: &str,
    method: &Method,
    written: &[Arc<str>],
    other_method: &Method,
    other_written: &[Arc<str>],
) -> ! {
    let this = spell_pattern(path, written);
    let other = spell_pattern(path, other_written);
    let name = other_written
        .iter()
        .zip(written)
        .find(|(other, this)| other != this)
        .map_or_else(String::new, |(other, _)| other.to_string());

    let reason = if method == other_method {
        format!(
            "A route parameter is matched by the position it sits at rather than by what it \
             is called, so this is that same route under a second name: mapping it replaces \
             the one above rather than adding one, and whatever answers binds `{name}`."
        )
    } else {
        format!(
            "A `HEAD` request that has no route of its own is answered by the `GET` route, so \
             the two describe one resource and name what identifies it once - `{name}`."
        )
    };

    panic!(
        "ambiguous route `{method} {this}`: `{other_method} {other}` is already mapped. \
         {reason} Name the parameter `{name}` here too, or tell the two routes apart with a \
         literal segment. Any other verb may name this position whatever it likes."
    );
}

/// A segment of a route as the tree holds it, on the way to an endpoint
enum PathSegment<'tree> {
    /// A literal segment
    Static(&'tree str),
    /// A parameter, under the name the tree binds it as
    Dynamic(&'tree str),
}

/// Spells the route reached through `segments`, with `names` at the positions its
/// parameters sit at when the endpoint carries names of its own
fn spell_route(segments: &[PathSegment<'_>], names: Option<&[Arc<str>]>) -> String {
    let mut path = String::new();
    let mut dynamic = 0;

    for segment in segments {
        path.push(PATH_SEPARATOR as char);
        match segment {
            PathSegment::Static(literal) => path.push_str(literal),
            PathSegment::Dynamic(bound) => {
                let name = names
                    .and_then(|names| names.get(dynamic))
                    .map_or(*bound, |name| name.as_ref());
                dynamic += 1;

                path.push(OPEN_BRACKET);
                path.push_str(name);
                path.push(CLOSE_BRACKET);
            }
        }
    }

    finish_path(path)
}

/// Spells `path` with `names` at the positions its parameters sit at
fn spell_pattern(path: &str, names: &[Arc<str>]) -> String {
    let mut pattern = String::with_capacity(path.len());
    let mut params = names.iter();

    for segment in split_path(path) {
        pattern.push(PATH_SEPARATOR as char);
        match is_dynamic_segment(segment).then(|| params.next()).flatten() {
            Some(name) => {
                pattern.push(OPEN_BRACKET);
                pattern.push_str(name);
                pattern.push(CLOSE_BRACKET);
            }
            None => pattern.push_str(segment),
        }
    }

    finish_path(pattern)
}

/// Returns `true` when `segment` names a route parameter rather than a literal segment.
#[inline(always)]
pub(crate) fn is_dynamic_segment(segment: &str) -> bool {
    segment.starts_with(OPEN_BRACKET) && segment.ends_with(CLOSE_BRACKET)
}

#[inline(always)]
pub(super) fn make_allowed_str<const N: usize>(
    handlers: &SmallVec<[RouteEndpoint; N]>,
) -> Arc<str> {
    if handlers.is_empty() {
        return Arc::from("");
    }

    // A GET route answers HEAD requests too, and `Allow` names the methods the resource
    // supports rather than the ones that were mapped
    let implied_head = handlers.iter().any(|h| h.method == Method::GET)
        && !handlers.iter().any(|h| h.method == Method::HEAD);

    let mut allowed = String::with_capacity(handlers.len() * DEFAULT_DEPTH);
    let mut iter = handlers
        .iter()
        .map(|h| h.method.as_str())
        .chain(implied_head.then_some(Method::HEAD.as_str()));
    if let Some(first) = iter.next() {
        allowed.push_str(first);
        for s in iter {
            allowed.push(ALLOW_METHOD_SEPARATOR);
            allowed.push_str(s);
        }
    }

    Arc::from(allowed)
}

/// Returns `true` if `path` already names a route the way the router reads it
///
/// Empty segments carry no meaning to [`RouteNode`] - `split_path` drops them - so a
/// path holding any is a second name for a route that already has one. A route is keyed
/// by the string it was written as in more places than the tree, and two names for one
/// route are two entries in every one of them.
#[inline]
pub(crate) fn is_canonical_path(path: &str) -> bool {
    path == ROOT_PATH
        || (path.starts_with(PATH_SEPARATOR as char)
            && !path.ends_with(PATH_SEPARATOR as char)
            && !path.contains(DOUBLE_PATH_SEPARATOR))
}

/// Names the route `path` names, the way the router reads it
#[inline]
pub(crate) fn canonical_path(path: &str) -> String {
    let mut canonical = String::with_capacity(path.len() + 1);

    write_path(&mut canonical, path);
    finish_path(canonical)
}

/// Joins a route group's prefix and a route's pattern into the name of the route they
/// address together
#[inline]
pub(crate) fn join_path(prefix: &str, pattern: &str) -> String {
    let mut path = String::with_capacity(prefix.len() + pattern.len() + 1);

    write_path(&mut path, prefix);
    write_path(&mut path, pattern);
    finish_path(path)
}

/// Appends the segments of `source` that name something
#[inline]
fn write_path(path: &mut String, source: &str) {
    for segment in split_path(source) {
        path.push(PATH_SEPARATOR as char);
        path.push_str(segment);
    }
}

/// Spells a path with no segments as the root, which is how every route names it
#[inline]
fn finish_path(mut path: String) -> String {
    if path.is_empty() {
        path.push(PATH_SEPARATOR as char);
    }
    path
}

/// Splits a path into the segments that name something, dropping the empty ones so that
/// `/x`, `/x/` and `//x` are read as the one path they are.
#[inline(always)]
pub(crate) fn split_path(path: &str) -> impl Iterator<Item = &str> {
    memchr_split_nonempty(PATH_SEPARATOR, path.as_bytes())
        .map(|s| std::str::from_utf8(s).expect("Invalid UTF-8 sequence in path"))
}

#[inline(always)]
fn method_order(method: &Method) -> u8 {
    match *method {
        Method::GET => 0,
        Method::POST => 1,
        Method::PUT => 2,
        Method::DELETE => 3,
        Method::PATCH => 4,
        Method::OPTIONS => 5,
        Method::HEAD => 6,
        Method::CONNECT => 7,
        Method::TRACE => 8,
        _ => 255,
    }
}

#[cfg(test)]
mod tests {
    use super::RouteEndpoint;
    use crate::http::endpoints::handlers::{Func, RouteHandler};
    use crate::http::endpoints::route::{
        DEFAULT_DEPTH, RouteNode, join_path, make_allowed_str, method_order, split_path,
    };
    use crate::ok;
    use hyper::Method;
    use smallvec::SmallVec;
    use std::sync::Arc;

    #[cfg(debug_assertions)]
    use super::super::meta::RouteInfo;

    #[test]
    fn it_inserts_and_finds_route() {
        let handler = || async { ok!() };
        let handler: RouteHandler = Func::new(handler);

        let path = "test";

        let mut route = RouteNode::new();
        route.insert(path, Method::GET, handler.into());

        let route_params = route.find(path);

        assert!(route_params.is_some());
    }

    #[test]
    fn it_inserts_and_finds_route_with_params() {
        let handler = || async { ok!() };
        let handler: RouteHandler = Func::new(handler);

        let path = "test/{value}";

        let mut route = RouteNode::new();
        route.insert(path, Method::GET, handler.into());

        let path = "test/some";

        let route_params = route.find(path).unwrap();
        let param = route_params.params.first().unwrap();

        assert_eq!(param.value.as_ref(), "some");
    }

    #[test]
    #[cfg(debug_assertions)]
    fn it_collects_single_static_route() {
        let handler = || async { ok!() };
        let handler: RouteHandler = Func::new(handler);

        let path = "/users";

        let mut route = RouteNode::new();
        route.insert(path, Method::GET, handler.into());

        let routes = route.collect();

        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0], (Method::GET, "/users"));
    }

    #[test]
    #[cfg(debug_assertions)]
    fn it_collects_multiple_methods_same_route() {
        let handler = || async { ok!() };
        let handler: RouteHandler = Func::new(handler);

        let path = "/users";

        let mut route = RouteNode::new();
        route.insert(path, Method::GET, handler.clone().into());
        route.insert(path, Method::POST, handler.into());

        let routes = route.collect();

        assert_eq!(routes.len(), 2);
        assert!(routes.contains(&RouteInfo::new(Method::GET, "/users")));
        assert!(routes.contains(&RouteInfo::new(Method::POST, "/users")));
    }

    #[test]
    #[cfg(debug_assertions)]
    fn it_collects_nested_static_routes() {
        let handler = || async { ok!() };
        let handler: RouteHandler = Func::new(handler);

        let path1 = "/users";
        let path2 = "/users/profile";

        let mut route = RouteNode::new();
        route.insert(path1, Method::GET, handler.clone().into());
        route.insert(path2, Method::GET, handler.into());

        let routes = route.collect();

        assert_eq!(routes.len(), 2);
        assert!(routes.contains(&RouteInfo::new(Method::GET, path1)));
        assert!(routes.contains(&RouteInfo::new(Method::GET, path2)));
    }

    #[test]
    #[cfg(debug_assertions)]
    fn it_collects_dynamic_routes() {
        let handler = || async { ok!() };
        let handler: RouteHandler = Func::new(handler);

        let path = "/users/{id}";

        let mut route = RouteNode::new();
        route.insert(path, Method::GET, handler.into());

        let routes = route.collect();

        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0], (Method::GET, path));
    }

    #[test]
    #[cfg(debug_assertions)]
    fn it_collects_mixed_static_and_dynamic_routes() {
        let handler = || async { ok!() };
        let handler: RouteHandler = Func::new(handler);

        let path1 = "/users";
        let path2 = "/users/{id}";
        let path3 = "/users/{id}/posts";

        let mut route = RouteNode::new();
        route.insert(path1, Method::GET, handler.clone().into());
        route.insert(path2, Method::GET, handler.clone().into());
        route.insert(path3, Method::GET, handler.into());

        let routes = route.collect();

        assert_eq!(routes.len(), 3);
        assert!(routes.contains(&RouteInfo::new(Method::GET, path1)));
        assert!(routes.contains(&RouteInfo::new(Method::GET, path2)));
        assert!(routes.contains(&RouteInfo::new(Method::GET, path3)));
    }

    #[test]
    #[cfg(debug_assertions)]
    fn it_collects_root_route() {
        let handler = || async { ok!() };
        let handler: RouteHandler = Func::new(handler);

        let path = "";

        let mut route = RouteNode::new();
        route.insert(path, Method::GET, handler.into());

        let routes = route.collect();

        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0], (Method::GET, "/"));
    }

    #[test]
    #[cfg(debug_assertions)]
    fn it_collects_complex_route_tree() {
        let handler = || async { ok!() };
        let handler: RouteHandler = Func::new(handler);

        let mut route = RouteNode::new();

        // Add various routes
        route.insert("/api/v1/users", Method::GET, handler.clone().into());
        route.insert("/api/v1/users", Method::POST, handler.clone().into());
        route.insert(
            "/api/v1/users/{id:integer}",
            Method::GET,
            handler.clone().into(),
        );
        route.insert("/api/v1/users/{id}", Method::PUT, handler.clone().into());
        route.insert("/api/v1/users/{id}", Method::DELETE, handler.clone().into());
        route.insert("/api/v1/posts", Method::GET, handler.clone().into());
        route.insert("/api/v2/users", Method::GET, handler.into());

        let routes = route.collect();

        assert_eq!(routes.len(), 7);
        assert!(routes.contains(&RouteInfo::new(Method::GET, "/api/v1/users")));
        assert!(routes.contains(&RouteInfo::new(Method::POST, "/api/v1/users")));
        assert!(routes.contains(&RouteInfo::new(Method::GET, "/api/v1/users/{id}")));
        assert!(routes.contains(&RouteInfo::new(Method::PUT, "/api/v1/users/{id}")));
        assert!(routes.contains(&RouteInfo::new(Method::DELETE, "/api/v1/users/{id}")));
        assert!(routes.contains(&RouteInfo::new(Method::GET, "/api/v1/posts")));
        assert!(routes.contains(&RouteInfo::new(Method::GET, "/api/v2/users")));
    }

    #[test]
    #[cfg(debug_assertions)]
    fn it_handles_empty_route_tree() {
        let route = RouteNode::new();

        let routes = route.collect();

        assert_eq!(routes.len(), 0);
    }

    #[test]
    #[cfg(debug_assertions)]
    fn it_collects_routes_with_multiple_dynamic_segments() {
        let handler = || async { ok!() };
        let handler: RouteHandler = Func::new(handler);

        let path = "/users/{userId}/posts/{postId}/comments";

        let mut route = RouteNode::new();
        route.insert(path, Method::GET, handler.into());

        let routes = route.collect();

        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0], (Method::GET, path));
    }

    #[test]
    #[cfg(debug_assertions)]
    fn it_collects_routes_with_different_http_methods() {
        let handler = || async { ok!() };
        let handler: RouteHandler = Func::new(handler);

        let path = "resource";

        let mut route = RouteNode::new();
        route.insert(path, Method::GET, handler.clone().into());
        route.insert(path, Method::POST, handler.clone().into());
        route.insert(path, Method::PUT, handler.clone().into());
        route.insert(path, Method::DELETE, handler.clone().into());
        route.insert(path, Method::PATCH, handler.into());

        let routes = route.collect();

        assert_eq!(routes.len(), 5);
        let methods: Vec<Method> = routes.iter().map(|r| r.method.clone()).collect();
        assert!(methods.contains(&Method::GET));
        assert!(methods.contains(&Method::POST));
        assert!(methods.contains(&Method::PUT));
        assert!(methods.contains(&Method::DELETE));
        assert!(methods.contains(&Method::PATCH));

        // All should have the same route
        for route in routes.iter() {
            assert_eq!(route.path, "/resource");
        }
    }

    #[test]
    fn in_check_method_order() {
        let methods = [
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::PATCH,
            Method::OPTIONS,
            Method::HEAD,
            Method::CONNECT,
            Method::TRACE,
        ];
        for i in 0..methods.len() - 1 {
            assert!(method_order(&methods[i]) < method_order(&methods[i + 1]));
        }
    }

    #[test]
    fn it_splits_path() {
        let path = "/a/b/c/d";
        let split = split_path(path);
        assert_eq!(split.collect::<Vec<_>>(), vec!["a", "b", "c", "d"])
    }

    #[test]
    fn it_splits_path_with_trailing_slash() {
        let path = "/a/b/c/d/";
        let split = split_path(path);
        assert_eq!(split.collect::<Vec<_>>(), vec!["a", "b", "c", "d"])
    }

    #[test]
    fn it_splits_path_without_leading_slash() {
        let path = "a/b/c/d";
        let split = split_path(path);
        assert_eq!(split.collect::<Vec<_>>(), vec!["a", "b", "c", "d"])
    }

    #[test]
    fn it_joins_a_prefix_and_a_pattern() {
        assert_eq!(join_path("/api", "/users"), "/api/users");
        assert_eq!(join_path("/api/v1", "/users/{id}"), "/api/v1/users/{id}");
    }

    #[test]
    fn it_joins_a_prefix_and_a_pattern_written_without_separators() {
        assert_eq!(join_path("api", "users"), "/api/users");
        assert_eq!(join_path("/api", "users"), "/api/users");
        assert_eq!(join_path("api/", "/users"), "/api/users");
    }

    #[test]
    fn it_drops_empty_segments_when_joining() {
        assert_eq!(join_path("/api/", "/users/"), "/api/users");
        assert_eq!(join_path("/api//", "//users//{id}//"), "/api/users/{id}");
    }

    #[test]
    fn it_joins_an_empty_prefix_or_pattern() {
        assert_eq!(join_path("", "/users"), "/users");
        assert_eq!(join_path("/api", ""), "/api");
        assert_eq!(join_path("/api", "/"), "/api");
    }

    /// The root is spelled the way a route mapped outside a group spells it, so both
    /// name one route rather than two.
    #[test]
    fn it_spells_the_root_the_way_every_other_route_does() {
        assert_eq!(join_path("", ""), "/");
        assert_eq!(join_path("/", "/"), "/");
        assert_eq!(join_path("/", "//"), "/");
    }

    #[test]
    fn it_keeps_typed_and_dynamic_segments_when_joining() {
        assert_eq!(
            join_path("/api", "/users/{id:integer}/roles/{role}"),
            "/api/users/{id:integer}/roles/{role}"
        );
    }

    /// The point of joining this way: paths that name one route read as one string, so
    /// anything keyed by that string counts the route once.
    #[test]
    fn it_reads_one_path_for_spellings_that_name_one_route() {
        assert_eq!(join_path("/api", "/hello"), join_path("/api", "/hello/"));
        assert_eq!(join_path("/api", "/hello"), join_path("/api/", "hello"));
        assert_eq!(join_path("/api", "/hello"), join_path("/api", "//hello"));
    }

    /// ... and the string it produces addresses the route the caller wrote.
    #[test]
    fn it_joins_a_path_that_finds_the_route_it_names() {
        let mut route = RouteNode::new();
        let handler: RouteHandler = Func::new(|| async { ok!() });

        route.insert(
            &join_path("/api/", "/users//{id}/"),
            Method::GET,
            handler.into(),
        );

        assert!(route.find("/api/users/7").is_some());
    }

    #[test]
    fn it_makes_allowed_str() {
        let handlers: SmallVec<[RouteEndpoint; DEFAULT_DEPTH]> = smallvec::smallvec![
            RouteEndpoint::new(Method::GET, None),
            RouteEndpoint::new(Method::HEAD, None),
        ];

        let allowed = make_allowed_str(&handlers);
        assert_eq!(allowed.as_ref(), "GET,HEAD");
    }

    #[test]
    fn it_makes_empty_allowed_str_if_no_handlers() {
        let handlers: SmallVec<[RouteEndpoint; DEFAULT_DEPTH]> = smallvec::smallvec![];
        let allowed = make_allowed_str(&handlers);
        assert_eq!(allowed.as_ref(), "");
    }

    #[test]
    #[should_panic(
        expected = "ambiguous route `GET /users/{name}`: `GET /users/{id}` is already mapped"
    )]
    fn it_rejects_a_second_name_for_one_parameter() {
        let handler: RouteHandler = Func::new(|| async { ok!() });

        let mut route = RouteNode::new();
        route.insert("/users/{id}", Method::GET, handler.clone().into());
        route.insert("/users/{name}", Method::GET, handler.into());
    }

    #[test]
    #[should_panic(expected = "ambiguous route `GET /{name}`: `GET /{id}` is already mapped")]
    fn it_names_the_root_position_of_a_conflict() {
        let handler: RouteHandler = Func::new(|| async { ok!() });

        let mut route = RouteNode::new();
        route.insert("/{id}", Method::GET, handler.clone().into());
        route.insert("/{name}", Method::GET, handler.into());
    }

    /// A `HEAD` request with no route of its own is answered by the `GET` route, so the two
    /// describe one resource and cannot disagree about what identifies it
    #[test]
    #[should_panic(
        expected = "ambiguous route `HEAD /users/{name}`: `GET /users/{id}` is already mapped"
    )]
    fn it_rejects_a_head_named_apart_from_the_get_it_answers_for() {
        let handler: RouteHandler = Func::new(|| async { ok!() });

        let mut route = RouteNode::new();
        route.insert("/users/{id}", Method::GET, handler.clone().into());
        route.insert("/users/{name}", Method::HEAD, handler.into());
    }

    /// ... and it reads the same way round
    #[test]
    #[should_panic(
        expected = "ambiguous route `GET /users/{name}`: `HEAD /users/{id}` is already mapped"
    )]
    fn it_rejects_a_get_named_apart_from_the_head_answering_for_it() {
        let handler: RouteHandler = Func::new(|| async { ok!() });

        let mut route = RouteNode::new();
        route.insert("/users/{id}", Method::HEAD, handler.clone().into());
        route.insert("/users/{name}", Method::GET, handler.into());
    }

    /// Another verb is another route, and it names what it reads for itself: reading a user
    /// by id and creating one by name meet at a position without describing one thing
    #[test]
    fn it_accepts_a_second_name_from_another_verb() {
        let handler: RouteHandler = Func::new(|| async { ok!() });

        let mut route = RouteNode::new();
        route.insert("/users/{id}", Method::GET, handler.clone().into());
        route.insert("/users/{name}", Method::POST, handler.into());

        let found = route.find("/users/42").unwrap();
        let handlers = found.route.handlers.as_ref().unwrap();

        // The tree binds the name the GET got there with, and only the POST carries one
        assert_eq!(found.params.first().unwrap().name.as_ref(), "id");
        assert!(handlers[0].params.is_none());
        assert_eq!(
            handlers[1].params.as_deref().unwrap(),
            [Arc::<str>::from("name")]
        );
    }

    /// Two routes on one verb parting at a position never meet at an endpoint, so each one
    /// keeps the name it was written with
    #[test]
    fn it_accepts_two_names_where_the_routes_part() {
        let handler: RouteHandler = Func::new(|| async { ok!() });

        let mut route = RouteNode::new();
        route.insert("/users/{id}/posts", Method::GET, handler.clone().into());
        route.insert("/users/{name}/comments", Method::GET, handler.into());

        let posts = route.find("/users/42/posts").unwrap();
        let comments = route.find("/users/42/comments").unwrap();

        assert!(posts.route.handlers.as_ref().unwrap()[0].params.is_none());
        assert_eq!(
            comments.route.handlers.as_ref().unwrap()[0]
                .params
                .as_deref()
                .unwrap(),
            [Arc::<str>::from("name")]
        );
    }

    /// The route listing is what the application was written as, so an endpoint naming a
    /// position for itself is listed under its own name rather than the tree's
    #[test]
    #[cfg(debug_assertions)]
    fn it_collects_each_route_under_the_name_it_was_written_with() {
        let handler: RouteHandler = Func::new(|| async { ok!() });

        let mut route = RouteNode::new();
        route.insert("/users/{id}", Method::GET, handler.clone().into());
        route.insert("/users/{name}", Method::POST, handler.clone().into());
        route.insert("/users/{name}/roles", Method::PUT, handler.into());

        let routes = route.collect();

        assert_eq!(routes.len(), 3);
        assert!(routes.contains(&RouteInfo::new(Method::GET, "/users/{id}")));
        assert!(routes.contains(&RouteInfo::new(Method::POST, "/users/{name}")));
        assert!(routes.contains(&RouteInfo::new(Method::PUT, "/users/{name}/roles")));
    }

    /// Mapping one route for several verbs is the whole point of the name matching, and a
    /// layer added to a route arrives here the same way a handler does
    #[test]
    fn it_accepts_the_same_parameter_name_again() {
        let handler: RouteHandler = Func::new(|| async { ok!() });

        let mut route = RouteNode::new();
        route.insert("/users/{id}", Method::GET, handler.clone().into());
        route.insert("/users/{id}", Method::HEAD, handler.clone().into());
        route.insert("/users/{id}", Method::POST, handler.clone().into());
        route.insert("/users/{id}/posts", Method::GET, handler.into());

        let found = route.find("/users/42").unwrap();

        assert_eq!(found.params.first().unwrap().name.as_ref(), "id");
        assert_eq!(found.route.allowed_methods().as_ref(), "GET,POST,HEAD");
        assert!(route.find("/users/42/posts").is_some());
    }

    /// The type a parameter is annotated with is not part of its name, so the two spell
    /// one parameter
    #[test]
    fn it_accepts_a_typed_spelling_of_a_name_already_registered() {
        let handler: RouteHandler = Func::new(|| async { ok!() });

        let mut route = RouteNode::new();
        route.insert("/users/{id}", Method::GET, handler.clone().into());
        route.insert("/users/{id:integer}", Method::POST, handler.into());

        assert!(route.find("/users/42").is_some());
    }

    /// A literal segment is matched before the parameter covering it, which is how a route
    /// is told apart from the parameter it sits under - and is the way out of a conflict
    #[test]
    fn it_accepts_a_literal_segment_beside_a_parameter() {
        let handler: RouteHandler = Func::new(|| async { ok!() });

        let mut route = RouteNode::new();
        route.insert("/users/{id}", Method::GET, handler.clone().into());
        route.insert("/users/me", Method::GET, handler.into());

        assert!(route.find("/users/me").unwrap().params.first().is_none());
        assert_eq!(
            route
                .find("/users/42")
                .unwrap()
                .params
                .first()
                .unwrap()
                .name
                .as_ref(),
            "id"
        );
    }
}
