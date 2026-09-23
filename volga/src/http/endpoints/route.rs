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
//! - **Literals win, but only where they lead somewhere:**
//!   A segment is read as a literal wherever one is mapped, so `/users/me` beats
//!   `/users/{id}`. A literal that turns out to be a dead end is given back: the lookup
//!   unwinds to the parameter it passed over and reads the segment again. Without that,
//!   mapping `/users/me/settings` would silently stop `/users/{id}` from answering
//!   `/users/me`.
//!
//! - **A catch-all is the last word at its position:**
//!   A catch-all parameter (`/files/{*path}`) reads the rest of the path, at least one
//!   segment of it, and is terminal by construction - it is not a child node, so nothing can
//!   be mapped below it. At every position a literal is tried first, a parameter second and
//!   the catch-all last, and the first difference from the left decides between two routes:
//!   `/assets/{*path}` answers `/assets/app.js` ahead of `/{lang}/{page}`, at any depth and
//!   in any registration order.
//!
//! ## Two passes
//!
//! The first pass is greedy: a literal where one is mapped, the parameter otherwise, and
//! no memory of what it passed over. When it ends on a mapped node, what it found is the
//! route the precedence above picks - an alternative it skipped is either a parameter where
//! it read a literal, or a catch-all, and both come later in that order. So a request that
//! takes this path pays nothing for backtracking or for catch-alls, whether or not any are
//! mapped.
//!
//! Only when the greedy walk dead-ends does the second pass run: a depth-first search in
//! the same order - literal, parameter, catch-all - that unwinds to the deepest alternative
//! first. That pass is what a request answered by a catch-all, a request answered by a
//! parameter behind a dead-end literal, and a request answered by nothing all pay for, on
//! top of the greedy walk that came before it.
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

use crate::http::endpoints::handlers::RouteHandler;
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
const CATCH_ALL_MARKER: char = '*';
const PATH_SEPARATOR: u8 = b'/';
const DOUBLE_PATH_SEPARATOR: &str = "//";
const ROOT_PATH: &str = "/";
const TYPE_SEPARATOR: char = ':';
const ALLOW_METHOD_SEPARATOR: char = ',';
const DEFAULT_DEPTH: usize = 4;

/// The route parameter names of one pattern, in the order the pattern writes them, while
/// the pattern is being read
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
    /// the common case and costs a request nothing.
    ///
    /// Boxed rather than inline: this list is read once per request and written once at
    /// startup, while the endpoint holding it is scanned by every request that reaches
    /// this node, so the two words a `Box` costs beat the ten a `SmallVec` would
    pub(super) params: Option<Box<[Arc<str>]>>,
    /// The CORS policy bound to this route, `None` while nothing has bound one
    #[cfg(feature = "middleware")]
    pub(super) cors: Option<CorsOverride>,
}

/// What answers every method at a resource no route is mapped at: the fallback of the route
/// group claiming the prefix the resource sits under
///
/// It carries what a [`RouteEndpoint`] carries but the method, since the method is exactly
/// what it does not look at.
#[derive(Clone)]
pub(super) struct Fallback {
    pub(super) pipeline: RoutePipeline,
    /// The parameter names the fallback's pattern was written with, as for
    /// [`RouteEndpoint::params`]
    pub(super) params: Option<Box<[Arc<str>]>>,
    /// The CORS policy bound to this fallback, `None` while nothing has bound one
    #[cfg(feature = "middleware")]
    pub(super) cors: Option<CorsOverride>,
    /// Set on the fallback a route group maps below its prefix, where the rest of the path
    /// is read by a catch-all the application never wrote. That binding is dropped before
    /// the request is labelled, so a fallback sees the parameters its prefix declares and
    /// nothing else - the same at the prefix and below it.
    pub(super) hides_tail: bool,
}

/// Represents route path node
#[derive(Clone)]
pub(super) struct RouteEntry {
    path: Arc<str>,
    node: Box<RouteNode>,
}

/// The endpoints answering one route, one per HTTP method
#[derive(Clone)]
pub(super) struct Resource {
    /// A list of associated endpoints for each HTTP method
    pub(super) handlers: Option<SmallVec<[RouteEndpoint; DEFAULT_DEPTH]>>,

    /// Cached allowed methods header value
    allowed_methods: Option<Arc<str>>,

    /// Set while the `GET` endpoint here is implicit: mapped by the framework on the
    /// application's behalf - the route the fallback file answers under a static file
    /// mount. It is left out of the route listing, and a `GET` route mapped by hand takes its
    /// place instead of being reported as a second name for it.
    ///
    /// Kept here rather than on the endpoint: an implicit endpoint is only ever a `GET`, and a
    /// flag on every endpoint would grow each of the ones a request scans - and the four held
    /// inline in every resource - for the sake of that one.
    pub(super) implicit_get: bool,

    /// What answers here while no route is mapped here for any method, if a route group
    /// claimed this position with a fallback of its own.
    ///
    /// A route mapped here for one method takes the whole position over: a request for
    /// another method is answered `405`, as it is at any route, rather than by the fallback.
    /// Boxed: most resources carry none, and a request reads it only once the handlers have
    /// turned out to be empty
    pub(super) fallback: Option<Box<Fallback>>,
}

/// A catch-all parameter: the route that reads the rest of a path from the position it
/// sits at
///
/// It holds a [`Resource`] rather than a [`RouteNode`], so a route continuing past a
/// catch-all has nowhere to be stored.
#[derive(Clone)]
struct CatchAll {
    /// The name the tree binds the rest of the path as
    name: Arc<str>,
    resource: Resource,
}

/// A node in the route tree
#[derive(Clone)]
pub(super) struct RouteNode {
    /// What answers a path that ends at this node
    resource: Resource,

    /// List of static routes
    static_routes: SmallVec<[RouteEntry; DEFAULT_DEPTH]>,

    /// Dynamic route
    dynamic_route: Option<RouteEntry>,

    /// The catch-all mapped at this position, if any.
    ///
    /// Boxed: most nodes carry none, and the greedy lookup never reads it, so it costs a
    /// node one word rather than the size of a [`Resource`]
    catch_all: Option<Box<CatchAll>>,
}

/// The parameters a backtracking search has bound so far, each as the name the tree binds
/// and the part of the path it reads - both borrowed, so giving a branch up costs nothing
type Bindings<'route, 'path> = SmallVec<[(&'route Arc<str>, &'path str); DEFAULT_DEPTH]>;

/// Parameters of a route
pub(super) struct RouteParams<'route> {
    pub(super) route: &'route Resource,
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
    fn new(method: Method, params: Option<Box<[Arc<str>]>>) -> Self {
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

impl Fallback {
    /// Creates a [`Fallback`] answering with `handler`
    #[inline]
    fn new(handler: RouteHandler, params: Option<Box<[Arc<str>]>>, hides_tail: bool) -> Self {
        Self {
            pipeline: Layer::Handler(handler).into(),
            params,
            #[cfg(feature = "middleware")]
            cors: None,
            hides_tail,
        }
    }

    /// Inserts middleware ahead of the layers this fallback already holds
    #[inline]
    #[cfg(feature = "middleware")]
    pub(super) fn prepend(&mut self, layers: &[MiddlewareFn]) {
        self.pipeline.prepend(layers);
    }
}

impl CatchAll {
    /// Creates a [`CatchAll`] binding the rest of a path as `name`
    #[inline]
    fn new(name: &str) -> Self {
        Self {
            name: Arc::from(name),
            resource: Resource::new(),
        }
    }
}

impl RouteNode {
    /// Create a new [`RouteNode`]
    #[inline]
    pub(super) fn new() -> Self {
        Self {
            resource: Resource::new(),
            static_routes: SmallVec::new(),
            dynamic_route: None,
            catch_all: None,
        }
    }

    /// Inserts a handler to the route tree
    ///
    /// # Panics
    /// if this route is a second name for one already mapped for `method`, or for the
    /// `GET` that a `HEAD` answers - see [`ambiguous_route`] - or if a catch-all parameter
    /// is followed by another segment - see [`misplaced_catch_all`].
    pub(super) fn insert(&mut self, path: &str, method: Method, handler: Layer) {
        let (resource, written, bound) = self.reach(path);
        resource.insert_handler(method, handler, &written, &bound, path);
    }

    /// Maps an [implicit](Resource::implicit_get) `GET` endpoint at `path`, unless a `GET`
    /// endpoint is mapped there already - by hand, or by an earlier call.
    #[cfg(feature = "static-files")]
    pub(super) fn insert_implicit(&mut self, path: &str, pipeline: RoutePipeline) {
        let (resource, written, bound) = self.reach(path);
        resource.insert_implicit(pipeline, &written, &bound);
    }

    /// Maps the fallback answering at `path`, replacing the one already mapped there.
    ///
    /// `hides_tail` says that `path` ends in a catch-all of the router's own - see
    /// [`Fallback::hides_tail`] - so the names it is labelled with leave that one out.
    pub(super) fn insert_fallback(&mut self, path: &str, handler: RouteHandler, hides_tail: bool) {
        let (resource, mut written, mut bound) = self.reach(path);

        if hides_tail {
            written.pop();
            bound.pop();
        }

        let params = (written != bound).then(|| Box::from(written.as_slice()));
        resource.fallback = Some(Box::new(Fallback::new(handler, params, hides_tail)));
    }

    /// Reaches the resource `path` names, creating the nodes on the way there, along with
    /// what the path calls its parameters and what the tree binds at the positions they sit
    /// at - the same names, unless another route reached a position first.
    ///
    /// # Panics
    /// if a catch-all parameter is followed by another segment - see [`misplaced_catch_all`].
    fn reach(&mut self, path: &str) -> (&mut Resource, ParamNames, ParamNames) {
        let mut current = self;
        let mut segments = split_path(path);

        let mut written = ParamNames::new();
        let mut bound = ParamNames::new();

        let resource = loop {
            let Some(segment) = segments.next() else {
                break &mut current.resource;
            };

            if !is_dynamic_segment(segment) {
                current = current.insert_static_node(segment);
                continue;
            }

            let name = param_name(segment);

            if is_catch_all_segment(segment) {
                if segments.next().is_some() {
                    misplaced_catch_all(path);
                }

                let (resource, name_bound) = current.insert_catch_all(name);
                bind_param(&mut written, &mut bound, name, name_bound);
                break resource;
            }

            let (next, name_bound) = current.insert_dynamic_node(name);
            bind_param(&mut written, &mut bound, name, name_bound);
            current = next;
        };

        (resource, written, bound)
    }

    /// Finds handlers by path
    ///
    /// A literal segment is read as a literal wherever one is mapped, and only falls back
    /// to the parameter sharing its position when the literal branch turns out to lead
    /// nowhere - either because the path parts ways deeper down, or because nothing is
    /// mapped where the path ends. Without that fallback a route like `/a/{b}` would
    /// stop answering `/a/b` the moment some unrelated route mapped `/a/b/c`. A catch-all
    /// is read after both, where neither leads anywhere.
    ///
    /// See [the module documentation](self) for why this takes two passes. The first one
    /// is written out here rather than chained to the second, which keeps the result it
    /// finds from being moved through an `Option` on the way out.
    #[inline]
    pub(super) fn find(&self, path: &str) -> Option<RouteParams<'_>> {
        // The first pass: the literal where one is mapped, the parameter otherwise, with no
        // record of what it passed over and no look at a catch-all
        let mut current = self;
        let mut params = PathArgs::new();

        for segment in split_path(path) {
            if let Ok(i) = current.static_routes.binary_search_by(|r| r.cmp(segment)) {
                current = current.static_routes[i].node.as_ref();
                continue;
            }

            let Some(next) = &current.dynamic_route else {
                return self.find_backtracking(path);
            };

            params.push(PathArg {
                name: Arc::clone(&next.path),
                value: Box::from(segment),
            });
            current = next.node.as_ref();
        }

        if !current.resource.is_mapped() {
            return self.find_backtracking(path);
        }

        Some(RouteParams {
            route: &current.resource,
            params,
        })
    }

    /// The second pass, for a path the greedy walk could not place: every reading of it in
    /// precedence order, until one of them reaches a mapped route.
    ///
    /// The search binds borrowed names and segments, and they are copied out only once a
    /// route is found - so a branch it gives up, and a path nothing answers, allocate
    /// nothing.
    #[inline(never)]
    fn find_backtracking(&self, path: &str) -> Option<RouteParams<'_>> {
        let mut bindings = Bindings::new();
        let route = self.search(path, split_path(path), &mut bindings)?;

        let params = bindings
            .into_iter()
            .map(|(name, value)| PathArg {
                name: Arc::clone(name),
                value: Box::from(value),
            })
            .collect();

        Some(RouteParams { route, params })
    }

    /// Searches this subtree for the route answering `segments`, the unread rest of `path`:
    /// the literal child first, the parameter child next and the catch-all last, so the walk
    /// unwinds to the deepest alternative it passed and gives up as little of the path as it
    /// has to. `bindings` is left as it was found unless a route is found.
    ///
    /// The recursion cannot blow up. It only descends into a child that exists, so it goes
    /// no deeper than the longest route mapped; and a node in this tree sits at one depth,
    /// reached only after exactly that many segments have been read, so no node is visited
    /// twice within a lookup. The work is bounded by the nodes the path can reach, never by
    /// the number of ways it could be read.
    fn search<'route, 'path, I>(
        &'route self,
        path: &'path str,
        mut segments: I,
        bindings: &mut Bindings<'route, 'path>,
    ) -> Option<&'route Resource>
    where
        I: Iterator<Item = &'path str> + Clone,
    {
        let Some(segment) = segments.next() else {
            return self.resource.is_mapped().then_some(&self.resource);
        };

        if let Ok(i) = self.static_routes.binary_search_by(|r| r.cmp(segment))
            && let Some(found) = self.static_routes[i]
                .node
                .search(path, segments.clone(), bindings)
        {
            return Some(found);
        }

        if let Some(dynamic) = &self.dynamic_route {
            let bound = bindings.len();
            bindings.push((&dynamic.path, segment));

            if let Some(found) = dynamic.node.search(path, segments, bindings) {
                return Some(found);
            }
            bindings.truncate(bound);
        }

        let catch_all = self
            .catch_all
            .as_deref()
            .filter(|catch_all| catch_all.resource.is_mapped())?;

        bindings.push((&catch_all.name, tail(path, segment)));

        Some(&catch_all.resource)
    }

    /// Finds the endpoints a route pattern names, reading the pattern the way it was written
    #[inline]
    #[cfg(feature = "middleware")]
    pub(super) fn find_mut(&mut self, pattern: &str) -> Option<&'_ mut Resource> {
        let mut current = self;
        let mut segments = split_path(pattern);

        let resource = loop {
            let Some(segment) = segments.next() else {
                break &mut current.resource;
            };

            if is_catch_all_segment(segment) {
                if segments.next().is_some() {
                    return None;
                }
                break &mut current.catch_all.as_mut()?.resource;
            }

            if let Ok(i) = current.static_routes.binary_search_by(|r| r.cmp(segment)) {
                current = current.static_routes[i].node.as_mut();
                continue;
            }

            if let Some(next) = &mut current.dynamic_route {
                current = next.node.as_mut();
                continue;
            }

            return None;
        };

        resource.is_mapped().then_some(resource)
    }

    #[cfg(feature = "middleware")]
    pub(super) fn compose(&mut self) {
        // Compose all static routes
        self.static_routes.iter_mut().for_each(|r| r.node.compose());

        // Compose a dynamic route if present
        if let Some(route) = self.dynamic_route.as_mut() {
            route.node.compose();
        }

        // Compose a catch-all route if present
        if let Some(catch_all) = self.catch_all.as_mut() {
            catch_all.resource.compose();
        }

        self.resource.compose();
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

        // Record the catch-all route (if any), which has nothing below it
        if let Some(catch_all) = &self.catch_all {
            segments.push(PathSegment::CatchAll(&catch_all.name));
            catch_all.resource.record_routes(routes, segments);
            segments.pop();
        }

        // Record handlers for this node
        self.resource.record_routes(routes, segments);
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

    /// Reaches the catch-all named `name` at this position, creating it when this is the
    /// first route to map one here, and hands back the name bound there - `name` itself,
    /// unless another route got here first and called it something else.
    #[inline(always)]
    fn insert_catch_all(&mut self, name: &str) -> (&mut Resource, Arc<str>) {
        let catch_all = self
            .catch_all
            .get_or_insert_with(|| Box::new(CatchAll::new(name)));
        let bound = Arc::clone(&catch_all.name);

        (&mut catch_all.resource, bound)
    }
}

impl Resource {
    /// Creates a [`Resource`] with nothing mapped
    #[inline]
    fn new() -> Self {
        Self {
            handlers: None,
            allowed_methods: None,
            implicit_get: false,
            fallback: None,
        }
    }

    /// Returns `true` when an endpoint is mapped here, for any method, or a fallback is
    #[inline(always)]
    fn is_mapped(&self) -> bool {
        self.endpoints().is_some() || self.fallback.is_some()
    }

    /// The endpoints mapped here, `None` while there are none
    #[inline(always)]
    pub(super) fn endpoints(&self) -> Option<&[RouteEndpoint]> {
        self.handlers.as_deref().filter(|h| !h.is_empty())
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

    /// Returns a mutable reference to the fallback mapped here
    #[inline]
    #[cfg(feature = "middleware")]
    pub(super) fn fallback_mut(&mut self) -> Option<&mut Fallback> {
        self.fallback.as_deref_mut()
    }

    #[cfg(feature = "middleware")]
    fn compose(&mut self) {
        if let Some(handlers) = self.handlers.as_mut() {
            handlers.iter_mut().for_each(|r| r.pipeline.compose());
        }
        if let Some(fallback) = self.fallback.as_mut() {
            fallback.pipeline.compose();
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

    /// Lists the routes answered here, reached through `segments`
    fn record_routes(
        &self,
        routes: &mut Vec<super::meta::RouteInfo>,
        segments: &[PathSegment<'_>],
    ) {
        let Some(ref handlers) = self.handlers else {
            return;
        };

        // An implicit endpoint was not written by the application, so it is not listed as
        // one of its routes - and nor is a fallback, which is not a route at all
        for handler in handlers
            .iter()
            .filter(|handler| !(self.implicit_get && handler.method == Method::GET))
        {
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
                conflicting_endpoint(handlers, self.implicit_get, &method, written, bound)
        {
            ambiguous_route(path, &method, written, &other_method, &other_written);
        }

        // A handler mapped by hand for `GET` takes the implicit endpoint's place below, and
        // the endpoint is the application's from then on
        if method == Method::GET && matches!(handler, Layer::Handler(_)) {
            self.implicit_get = false;
        }

        // A pattern naming its parameters the way the tree already binds them - the route
        // that reached each position first, and every route agreeing with it - says
        // nothing, and a request to it is labelled straight from the tree
        let params = (written != bound).then(|| Box::from(written.as_slice()));
        let handlers = self.handlers.get_or_insert_with(SmallVec::new);

        let endpoint = match handlers.binary_search_by(|r| r.cmp(&method)) {
            Ok(i) => {
                // Mapping a handler where one is already mapped replaces the route, and
                // takes with it every layer bound to the registration being replaced: a
                // handler and its middleware are written together, and answering with one
                // while running the other's middleware is a pipeline nobody wrote.
                // Layers added to a route do not replace it - only another handler does.
                // An implicit endpoint is taken over the same way
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

    /// Maps an implicit `GET` endpoint here, unless a `GET` endpoint is mapped already.
    ///
    /// Nothing is reported either way: an implicit endpoint only answers what the
    /// application left unanswered, so one mapped by hand keeps its place, and an implicit
    /// one already here answers exactly what a second would.
    #[cfg(feature = "static-files")]
    fn insert_implicit(
        &mut self,
        pipeline: RoutePipeline,
        written: &ParamNames,
        bound: &ParamNames,
    ) {
        let handlers = self.handlers.get_or_insert_with(SmallVec::new);
        let Err(i) = handlers.binary_search_by(|r| r.cmp(&Method::GET)) else {
            return;
        };

        let mut endpoint = RouteEndpoint::new(
            Method::GET,
            (written != bound).then(|| Box::from(written.as_slice())),
        );
        endpoint.pipeline = pipeline;

        handlers.insert(i, endpoint);
        self.allowed_methods = Some(make_allowed_str(handlers));
        self.implicit_get = true;
    }
}

/// Records a parameter a pattern calls `name`, where the tree binds `name_bound`
#[inline(always)]
fn bind_param(written: &mut ParamNames, bound: &mut ParamNames, name: &str, name_bound: Arc<str>) {
    written.push(if name_bound.as_ref() == name {
        Arc::clone(&name_bound)
    } else {
        Arc::from(name)
    });
    bound.push(name_bound);
}

/// The name a `{name}`, `{name:type}` or `{*name}` segment binds its value as
#[inline(always)]
pub(crate) fn param_name(segment: &str) -> &str {
    // expects a placeholder but safely handles unexpected input
    let Some(inner) = segment
        .strip_prefix(OPEN_BRACKET)
        .and_then(|s| s.strip_suffix(CLOSE_BRACKET))
    else {
        return segment;
    };

    let inner = inner.strip_prefix(CATCH_ALL_MARKER).unwrap_or(inner);
    let (name, _) = inner.split_once(TYPE_SEPARATOR).unwrap_or((inner, ""));
    name
}

/// The rest of `path` from `segment` on, where `segment` is one [`split_path`] read out of
/// `path` - which is what a catch-all binds: every segment from the position it sits at to
/// the end, with the separators between them, and a trailing one, as the request wrote them
#[inline]
fn tail<'path>(path: &'path str, segment: &'path str) -> &'path str {
    let start = segment.as_ptr() as usize - path.as_ptr() as usize;
    &path[start..]
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
///
/// An implicit endpoint - the `GET` one, while `implicit_get` is set - conflicts with
/// nothing: the application never wrote its pattern, so there is no name of its own to
/// contradict, and a route mapped by hand for its method takes its place.
#[inline]
fn conflicting_endpoint(
    handlers: &[RouteEndpoint],
    implicit_get: bool,
    method: &Method,
    written: &ParamNames,
    bound: &ParamNames,
) -> Option<(Method, ParamNames)> {
    handlers
        .iter()
        .find(|endpoint| {
            !(implicit_get && endpoint.method == Method::GET)
                && (endpoint.method == *method || answers_for(&endpoint.method, method))
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

/// Reports a catch-all parameter written anywhere but at the end of a route
///
/// A catch-all reads the rest of the path, so a segment after it could never be read -
/// the tree has nowhere to store one, and quietly dropping it would map a route nobody
/// wrote.
#[cold]
#[inline(never)]
fn misplaced_catch_all(path: &str) -> ! {
    panic!(
        "invalid route `{path}`: a catch-all parameter reads the rest of the path, so it can \
         only be the last segment of a route. Move it to the end, or map what follows it as \
         a route of its own."
    );
}

/// A segment of a route as the tree holds it, on the way to an endpoint
enum PathSegment<'tree> {
    /// A literal segment
    Static(&'tree str),
    /// A parameter, under the name the tree binds it as
    Dynamic(&'tree str),
    /// A catch-all parameter, under the name the tree binds it as
    CatchAll(&'tree str),
}

/// Spells the route reached through `segments`, with `names` at the positions its
/// parameters sit at when the endpoint carries names of its own
fn spell_route(segments: &[PathSegment<'_>], names: Option<&[Arc<str>]>) -> String {
    let mut path = String::new();
    let mut params = 0;

    for segment in segments {
        path.push(PATH_SEPARATOR as char);

        let (bound, catch_all) = match segment {
            PathSegment::Static(literal) => {
                path.push_str(literal);
                continue;
            }
            PathSegment::Dynamic(bound) => (*bound, false),
            PathSegment::CatchAll(bound) => (*bound, true),
        };

        let name = names
            .and_then(|names| names.get(params))
            .map_or(bound, |name| name.as_ref());
        params += 1;

        spell_param(&mut path, name, catch_all);
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
            Some(name) => spell_param(&mut pattern, name, is_catch_all_segment(segment)),
            None => pattern.push_str(segment),
        }
    }

    finish_path(pattern)
}

/// Spells a parameter placeholder, `{name}` or `{*name}`
#[inline]
fn spell_param(path: &mut String, name: &str, catch_all: bool) {
    path.push(OPEN_BRACKET);
    if catch_all {
        path.push(CATCH_ALL_MARKER);
    }
    path.push_str(name);
    path.push(CLOSE_BRACKET);
}

/// Returns `true` when `segment` names a route parameter rather than a literal segment.
///
/// A catch-all parameter is a route parameter too.
#[inline(always)]
pub(crate) fn is_dynamic_segment(segment: &str) -> bool {
    segment.starts_with(OPEN_BRACKET) && segment.ends_with(CLOSE_BRACKET)
}

/// Returns `true` when `segment` names a catch-all parameter, `{*name}`.
#[inline(always)]
pub(crate) fn is_catch_all_segment(segment: &str) -> bool {
    segment
        .strip_prefix(OPEN_BRACKET)
        .is_some_and(|inner| inner.starts_with(CATCH_ALL_MARKER))
        && segment.ends_with(CLOSE_BRACKET)
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

/// Returns `true` when `left` and `right` name one position in the tree
///
/// A parameter is matched by the position it sits at rather than by what it is called or
/// what it is typed as, so `/{tenant}/{*rest}`, `/{org}/{*path}` and `/{id:integer}/{*rest}`
/// all reach one resource - and anything keyed by a pattern has to count them as one.
#[cfg(any(feature = "middleware", feature = "openapi"))]
pub(crate) fn same_position(left: &str, right: &str) -> bool {
    let mut left = split_path(left);
    let mut right = split_path(right);

    loop {
        match (left.next(), right.next()) {
            (None, None) => return true,
            (Some(left), Some(right)) if same_segment(left, right) => continue,
            _ => return false,
        }
    }
}

/// Returns `true` when two segments of a pattern occupy one position: the same literal, two
/// parameters, or two catch-alls
#[inline]
#[cfg(any(feature = "middleware", feature = "openapi"))]
fn same_segment(left: &str, right: &str) -> bool {
    match (is_dynamic_segment(left), is_dynamic_segment(right)) {
        (true, true) => is_catch_all_segment(left) == is_catch_all_segment(right),
        (false, false) => left == right,
        _ => false,
    }
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

/// Spells a route pattern without the type annotations its parameters carry, which is how
/// the router reads it: `/users/{id:integer}` and `/users/{id}` name one route
#[inline]
#[cfg(feature = "openapi")]
pub(crate) fn untyped_path(pattern: &str) -> std::borrow::Cow<'_, str> {
    use std::borrow::Cow;

    let typed = split_path(pattern)
        .any(|segment| is_dynamic_segment(segment) && segment.contains(TYPE_SEPARATOR));

    if !typed {
        return Cow::Borrowed(pattern);
    }

    let mut path = String::with_capacity(pattern.len());
    for segment in split_path(pattern) {
        path.push(PATH_SEPARATOR as char);
        if is_dynamic_segment(segment) {
            spell_param(
                &mut path,
                param_name(segment),
                is_catch_all_segment(segment),
            );
        } else {
            path.push_str(segment);
        }
    }

    Cow::Owned(finish_path(path))
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
pub(crate) fn split_path(path: &str) -> impl Iterator<Item = &str> + Clone {
    memchr_split_nonempty(PATH_SEPARATOR, path)
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
    #[cfg(feature = "openapi")]
    fn it_spells_a_pattern_without_parameter_types() {
        use super::untyped_path;
        use std::borrow::Cow;

        assert_eq!(
            untyped_path("/users/{id:integer}/files/{*path:string}"),
            "/users/{id}/files/{*path}"
        );
        assert!(matches!(
            untyped_path("/users/{id}"),
            Cow::Borrowed("/users/{id}")
        ));
        // A literal carrying the separator is a literal, not a type annotation
        assert!(matches!(
            untyped_path("/at/12:00"),
            Cow::Borrowed("/at/12:00")
        ));
    }

    #[test]
    #[cfg(any(feature = "middleware", feature = "openapi"))]
    fn it_reads_one_position_under_any_spelling_of_its_parameters() {
        use super::same_position;

        for (left, right) in [
            ("/api/{tenant}", "/api/{org}"),
            ("/api/{tenant}/{*rest}", "/api/{org}/{*path}"),
            ("/api/{id:integer}/{*rest}", "/api/{id}/{*rest}"),
            ("/api/", "//api"),
            ("/", ""),
        ] {
            assert!(same_position(left, right), "{left} {right}");
        }

        for (left, right) in [
            ("/api/{id}", "/api/{*rest}"),
            ("/api/{id}", "/api/id"),
            ("/api", "/api/{*rest}"),
            ("/api/v1", "/api/v2"),
        ] {
            assert!(!same_position(left, right), "{left} {right}");
        }
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

    /// Collects the parameters a lookup bound, as `(name, value)` pairs.
    fn bound(route: &RouteNode, path: &str) -> Option<Vec<(String, String)>> {
        route.find(path).map(|found| {
            found
                .params
                .iter()
                .map(|p| (p.name.to_string(), p.value.to_string()))
                .collect()
        })
    }

    #[test]
    fn it_falls_back_to_a_parameter_when_the_literal_branch_ends_short() {
        let handler: RouteHandler = Func::new(|| async { ok!() });

        let mut route = RouteNode::new();
        route.insert("/a/b/c/d/e/i/k", Method::GET, handler.clone().into());
        route.insert("/a/{b}", Method::GET, handler.into());

        // `b` is a literal on the way to /a/b/c/d/e/i/k, but nothing is mapped at
        // /a/b itself, so the walk has to come back and read it as the parameter
        assert_eq!(
            bound(&route, "/a/b"),
            Some(vec![("b".to_string(), "b".to_string())])
        );
        assert_eq!(bound(&route, "/a/b/c/d/e/i/k"), Some(vec![]));
        assert_eq!(
            bound(&route, "/a/z"),
            Some(vec![("b".to_string(), "z".to_string())])
        );
    }

    #[test]
    fn it_falls_back_to_a_parameter_when_the_literal_branch_parts_deeper() {
        let handler: RouteHandler = Func::new(|| async { ok!() });

        let mut route = RouteNode::new();
        route.insert("/users/me/settings", Method::GET, handler.clone().into());
        route.insert("/users/{id}/posts", Method::GET, handler.into());

        // The literal `me` matches, and only the segment after it parts ways
        assert_eq!(
            bound(&route, "/users/me/posts"),
            Some(vec![("id".to_string(), "me".to_string())])
        );
        assert_eq!(bound(&route, "/users/me/settings"), Some(vec![]));
        assert_eq!(bound(&route, "/users/me/unmapped"), None);
    }

    #[test]
    fn it_backtracks_to_the_nearest_parameter_first() {
        let handler: RouteHandler = Func::new(|| async { ok!() });

        let mut route = RouteNode::new();
        route.insert("/a/b/c/d", Method::GET, handler.clone().into());
        route.insert("/a/b/{y}/e", Method::GET, handler.clone().into());
        route.insert("/a/{x}/z", Method::GET, handler.into());

        // Two parameters were passed over on the way down; the deeper one is the
        // one that gets to read its segment first
        assert_eq!(
            bound(&route, "/a/b/c/e"),
            Some(vec![("y".to_string(), "c".to_string())])
        );
        // Nothing under `b` fits, so the walk unwinds all the way to `{x}`
        assert_eq!(
            bound(&route, "/a/b/z"),
            Some(vec![("x".to_string(), "b".to_string())])
        );
        // And when no parameter fits either, the lookup still misses
        assert_eq!(bound(&route, "/a/b/q/w"), None);
    }

    #[test]
    fn it_reads_a_literal_before_a_parameter_that_also_fits() {
        let handler: RouteHandler = Func::new(|| async { ok!() });

        let mut route = RouteNode::new();
        route.insert("/users/{id}", Method::GET, handler.clone().into());
        route.insert("/users/me", Method::GET, handler.into());

        assert_eq!(bound(&route, "/users/me"), Some(vec![]));
        assert_eq!(
            bound(&route, "/users/42"),
            Some(vec![("id".to_string(), "42".to_string())])
        );
    }

    #[test]
    fn it_keeps_a_literal_mapped_for_another_verb_over_a_parameter() {
        let handler: RouteHandler = Func::new(|| async { ok!() });

        let mut route = RouteNode::new();
        route.insert("/users/me", Method::POST, handler.clone().into());
        route.insert("/users/{id}", Method::GET, handler.into());

        // The literal node carries a handler, just not for every verb. Reading it
        // as the parameter instead would answer a request that belongs to the
        // literal route, and would turn its 405 into someone else's 200
        assert_eq!(bound(&route, "/users/me"), Some(vec![]));
    }

    /// Builds a tree mapping every pattern in `patterns` for `GET`, in that order.
    fn tree(patterns: &[&str]) -> RouteNode {
        let handler: RouteHandler = Func::new(|| async { ok!() });

        let mut route = RouteNode::new();
        for pattern in patterns {
            route.insert(pattern, Method::GET, handler.clone().into());
        }
        route
    }

    /// `(name, value)` pairs, spelled briefly.
    fn args(pairs: &[(&str, &str)]) -> Option<Vec<(String, String)>> {
        Some(
            pairs
                .iter()
                .map(|(name, value)| (name.to_string(), value.to_string()))
                .collect(),
        )
    }

    #[test]
    fn it_binds_the_rest_of_the_path_to_a_catch_all() {
        let route = tree(&["/files/{*path}"]);

        assert_eq!(bound(&route, "/files/a"), args(&[("path", "a")]));
        assert_eq!(
            bound(&route, "/files/a/b/c.txt"),
            args(&[("path", "a/b/c.txt")])
        );
    }

    /// A catch-all reads at least one segment, so the position it sits at can carry a
    /// route of its own
    #[test]
    fn it_does_not_bind_an_empty_tail() {
        let route = tree(&["/files/{*path}", "/{*all}"]);

        assert_eq!(bound(&route, "/files"), args(&[("all", "files")]));
        assert_eq!(bound(&route, "/files/"), args(&[("all", "files/")]));
        assert_eq!(bound(&route, "/"), None);
        assert_eq!(bound(&route, ""), None);
    }

    #[test]
    fn it_answers_the_position_of_a_catch_all_with_a_route_mapped_there() {
        let route = tree(&["/files/{*path}", "/files"]);

        assert_eq!(bound(&route, "/files"), Some(vec![]));
        assert_eq!(bound(&route, "/files/a"), args(&[("path", "a")]));
    }

    /// The tail is the path as the request wrote it, from the first segment the catch-all
    /// reads - the separators inside it and after it included, so a proxy can forward it
    #[test]
    fn it_binds_the_tail_as_the_request_wrote_it() {
        let route = tree(&["/files/{*path}"]);

        assert_eq!(bound(&route, "/files/a/b/"), args(&[("path", "a/b/")]));
        assert_eq!(bound(&route, "/files/a//b"), args(&[("path", "a//b")]));
        assert_eq!(bound(&route, "/files//a"), args(&[("path", "a")]));
        assert_eq!(bound(&route, "//files/a"), args(&[("path", "a")]));
        assert_eq!(
            bound(&route, "/files/a%2Fb/c"),
            args(&[("path", "a%2Fb/c")])
        );
        // Not a file system path: nothing in it is resolved on the way
        assert_eq!(
            bound(&route, "/files/../../etc/passwd"),
            args(&[("path", "../../etc/passwd")])
        );
    }

    #[test]
    fn it_binds_the_parameters_before_a_catch_all() {
        let route = tree(&["/users/{id}/files/{*path}"]);

        assert_eq!(
            bound(&route, "/users/7/files/a/b"),
            args(&[("id", "7"), ("path", "a/b")])
        );
        assert_eq!(bound(&route, "/users/7/files"), None);
    }

    #[test]
    fn it_reads_a_literal_before_a_catch_all() {
        let route = tree(&["/{*path}", "/api/users"]);

        assert_eq!(bound(&route, "/api/users"), Some(vec![]));
        // The literal branch dead-ends, and the catch-all it passed over reads it all
        assert_eq!(
            bound(&route, "/api/unknown"),
            args(&[("path", "api/unknown")])
        );
        assert_eq!(bound(&route, "/api"), args(&[("path", "api")]));
    }

    #[test]
    fn it_reads_a_parameter_before_a_catch_all() {
        let route = tree(&["/users/{*rest}", "/users/{id}"]);

        assert_eq!(bound(&route, "/users/1"), args(&[("id", "1")]));
        assert_eq!(bound(&route, "/users/1/2"), args(&[("rest", "1/2")]));
    }

    /// The parameter bound on the way into a branch that dead-ends is not left behind for
    /// the catch-all that answers instead
    #[test]
    fn it_unbinds_a_parameter_whose_branch_dead_ends_before_a_catch_all() {
        let route = tree(&["/files/{id}/meta", "/files/{*path}"]);

        assert_eq!(bound(&route, "/files/5/meta"), args(&[("id", "5")]));
        assert_eq!(
            bound(&route, "/files/5/other"),
            args(&[("path", "5/other")])
        );
        assert_eq!(bound(&route, "/files/5"), args(&[("path", "5")]));
    }

    /// The first position two routes differ at decides between them, whatever order they
    /// were mapped in and however deep the path goes: a literal there beats a parameter
    #[test]
    fn it_decides_between_routes_at_the_first_position_they_differ() {
        for patterns in [
            ["/assets/{*path}", "/{lang}/{page}"],
            ["/{lang}/{page}", "/assets/{*path}"],
        ] {
            let route = tree(&patterns);

            assert_eq!(
                bound(&route, "/assets/app.js"),
                args(&[("path", "app.js")]),
                "{patterns:?}"
            );
            assert_eq!(
                bound(&route, "/assets/css/app.css"),
                args(&[("path", "css/app.css")]),
                "{patterns:?}"
            );
            assert_eq!(
                bound(&route, "/en/home"),
                args(&[("lang", "en"), ("page", "home")]),
                "{patterns:?}"
            );
        }
    }

    #[test]
    fn it_prefers_a_catch_all_under_a_literal_to_a_deeper_one_under_a_parameter() {
        let route = tree(&["/{id}/x/{*rest}", "/files/{*path}"]);

        assert_eq!(bound(&route, "/files/x/y"), args(&[("path", "x/y")]));
        assert_eq!(
            bound(&route, "/other/x/y"),
            args(&[("id", "other"), ("rest", "y")])
        );
    }

    #[test]
    fn it_prefers_the_deeper_of_two_catch_alls_on_one_branch() {
        let route = tree(&["/{*path}", "/api/{*rest}"]);

        assert_eq!(bound(&route, "/api/x/y"), args(&[("rest", "x/y")]));
        assert_eq!(bound(&route, "/api"), args(&[("path", "api")]));
        assert_eq!(bound(&route, "/other/x"), args(&[("path", "other/x")]));
    }

    #[test]
    fn it_reads_a_typed_catch_all_by_its_name() {
        let route = tree(&["/files/{*path:string}"]);

        assert_eq!(bound(&route, "/files/a/b"), args(&[("path", "a/b")]));
    }

    #[test]
    #[should_panic(
        expected = "invalid route `/{*path}/edit`: a catch-all parameter reads the rest of the path"
    )]
    fn it_rejects_a_segment_after_a_catch_all() {
        tree(&["/{*path}/edit"]);
    }

    #[test]
    #[should_panic(
        expected = "ambiguous route `GET /files/{*rest}`: `GET /files/{*path}` is already mapped"
    )]
    fn it_rejects_a_second_name_for_one_catch_all() {
        tree(&["/files/{*path}", "/files/{*rest}"]);
    }

    #[test]
    fn it_accepts_a_second_name_for_a_catch_all_from_another_verb() {
        let handler: RouteHandler = Func::new(|| async { ok!() });

        let mut route = RouteNode::new();
        route.insert("/files/{*path}", Method::GET, handler.clone().into());
        route.insert("/files/{*rest}", Method::POST, handler.into());

        let found = route.find("/files/a/b").unwrap();
        let handlers = found.route.handlers.as_ref().unwrap();

        assert_eq!(found.params.first().unwrap().name.as_ref(), "path");
        assert!(handlers[0].params.is_none());
        assert_eq!(
            handlers[1].params.as_deref().unwrap(),
            [Arc::<str>::from("rest")]
        );
        assert_eq!(found.route.allowed_methods().as_ref(), "GET,POST,HEAD");
    }

    #[test]
    #[cfg(debug_assertions)]
    fn it_collects_catch_all_routes() {
        let handler: RouteHandler = Func::new(|| async { ok!() });

        let mut route = RouteNode::new();
        route.insert("/files/{*path}", Method::GET, handler.clone().into());
        route.insert("/files/{*rest}", Method::PUT, handler.clone().into());
        route.insert("/users/{id}/files/{*path}", Method::GET, handler.into());

        let routes = route.collect();

        assert_eq!(routes.len(), 3);
        assert!(routes.contains(&RouteInfo::new(Method::GET, "/files/{*path}")));
        assert!(routes.contains(&RouteInfo::new(Method::PUT, "/files/{*rest}")));
        assert!(routes.contains(&RouteInfo::new(Method::GET, "/users/{id}/files/{*path}")));
    }

    #[test]
    #[cfg(feature = "middleware")]
    fn it_finds_a_catch_all_route_by_its_pattern() {
        let mut route = tree(&["/files/{*path}", "/files/{id}"]);

        assert!(route.find_mut("/files/{*path}").is_some());
        assert!(route.find_mut("/files/{id}").is_some());
        assert!(route.find_mut("/users/{*path}").is_none());
        assert!(route.find_mut("/files/{*path}/edit").is_none());
    }

    /// Maps an implicit `GET` endpoint at `path`
    #[cfg(feature = "static-files")]
    fn insert_implicit(route: &mut RouteNode, path: &str) {
        use super::{Layer, RoutePipeline};

        let handler: RouteHandler = Func::new(|| async { ok!() });
        route.insert_implicit(path, RoutePipeline::from(Layer::from(handler)));
    }

    /// Whether the `GET` endpoint answering `path` is an implicit one
    #[cfg(feature = "static-files")]
    fn answered_implicitly(route: &RouteNode, path: &str) -> bool {
        let found = route.find(path).expect("a route answers");
        assert!(
            found.route.handler(&Method::GET).is_some(),
            "a GET endpoint answers"
        );

        found.route.implicit_get
    }

    /// A route mapped by hand where an implicit one is takes its place - even under another
    /// name, which would be a second name for a route mapped by hand
    #[test]
    #[cfg(feature = "static-files")]
    fn it_gives_an_implicit_endpoint_up_to_a_route_mapped_by_hand() {
        let handler: RouteHandler = Func::new(|| async { ok!() });

        let mut route = RouteNode::new();
        insert_implicit(&mut route, "/{*path}");
        route.insert("/{*rest}", Method::GET, handler.into());

        assert!(!answered_implicitly(&route, "/a/b"));
        assert_eq!(
            route
                .find("/a/b")
                .unwrap()
                .route
                .handlers
                .as_ref()
                .unwrap()
                .len(),
            1
        );
    }

    /// ... and an implicit endpoint mapped after a route mapped by hand leaves it in place
    #[test]
    #[cfg(feature = "static-files")]
    fn it_keeps_a_route_mapped_by_hand_over_an_implicit_endpoint() {
        let mut route = tree(&["/{*rest}", "/"]);
        insert_implicit(&mut route, "/{*path}");
        insert_implicit(&mut route, "/");

        assert!(!answered_implicitly(&route, "/a/b"));
        assert!(!answered_implicitly(&route, "/"));
    }

    /// An implicit endpoint shares its position with the other verbs mapped there, and the
    /// `Allow` header names them all
    #[test]
    #[cfg(feature = "static-files")]
    fn it_lets_an_implicit_endpoint_share_its_position_with_another_verb() {
        let handler: RouteHandler = Func::new(|| async { ok!() });

        let mut route = RouteNode::new();
        route.insert("/{*rest}", Method::POST, handler.clone().into());
        insert_implicit(&mut route, "/{*path}");
        route.insert("/{*other}", Method::HEAD, handler.into());

        let found = route.find("/a/b").unwrap();

        assert!(answered_implicitly(&route, "/a/b"));
        assert_eq!(found.route.allowed_methods().as_ref(), "GET,POST,HEAD");
    }

    /// The route listing is what the application was written as, and an implicit endpoint
    /// was not written by it
    #[test]
    #[cfg(all(feature = "static-files", debug_assertions))]
    fn it_leaves_an_implicit_endpoint_out_of_the_route_listing() {
        let mut route = tree(&["/users"]);
        insert_implicit(&mut route, "/");
        insert_implicit(&mut route, "/{*path}");

        let routes = route.collect();

        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0], (Method::GET, "/users"));
    }

    /// Maps a fallback at `path`
    fn insert_fallback(route: &mut RouteNode, path: &str) {
        route.insert_fallback(path, Func::new(|| async { ok!() }), false);
    }

    #[test]
    fn it_finds_a_position_only_a_fallback_is_mapped_at() {
        let mut route = tree(&["/api/models"]);
        insert_fallback(&mut route, "/api");
        insert_fallback(&mut route, "/api/{*rest}");

        for path in ["/api", "/api/", "/api/nope", "/api/models/7"] {
            let found = route.find(path).expect(path);

            assert!(found.route.endpoints().is_none(), "{path}");
            assert!(found.route.fallback.is_some(), "{path}");
        }

        // A route answers its own position, and the fallback is not consulted there
        let models = route.find("/api/models").unwrap();
        assert!(models.route.endpoints().is_some());
        assert!(models.route.fallback.is_none());

        assert!(route.find("/other").is_none());
    }

    /// A fallback under a literal prefix is read before a parameter route that would read
    /// the same segments, since the literal comes first at the position they part at
    #[test]
    fn it_reads_a_fallback_under_a_literal_before_a_parameter() {
        let mut route = tree(&["/{lang}/{page}", "/{*path}"]);
        insert_fallback(&mut route, "/api/{*rest}");

        assert_eq!(bound(&route, "/api/nope"), args(&[("rest", "nope")]));
        assert_eq!(
            bound(&route, "/en/home"),
            args(&[("lang", "en"), ("page", "home")])
        );
        assert_eq!(bound(&route, "/api/a/b"), args(&[("rest", "a/b")]));
    }

    #[test]
    #[cfg(debug_assertions)]
    fn it_leaves_a_fallback_out_of_the_route_listing() {
        let mut route = tree(&["/api/models"]);
        insert_fallback(&mut route, "/api");
        insert_fallback(&mut route, "/api/{*rest}");

        let routes = route.collect();

        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0], (Method::GET, "/api/models"));
    }
}
