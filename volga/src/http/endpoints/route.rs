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
//!   overhead in the common case of static routing.
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

use hyper::Method;
use smallvec::SmallVec;
use crate::utils::str::memchr_split_nonempty;

#[cfg(feature = "middleware")]
use {
    crate::http::cors::CorsHeaders,
    std::sync::Arc
};

pub(crate) use path_args::{PathArgs, PathArg};
pub(crate) use layer::{RoutePipeline, Layer};

pub(crate) mod path_args;
pub(crate) mod layer;

const OPEN_BRACKET: char = '{';
const CLOSE_BRACKET: char = '}';
const PATH_SEPARATOR: u8 = b'/';
const ALLOW_METHOD_SEPARATOR: char = ',';
const DEFAULT_DEPTH: usize = 4;

/// Represents a full route's "local" middleware pipeline
/// with handler 
#[derive(Clone)]
pub(super) struct RouteEndpoint {
    pub(super) method: Method,
    pub(super) pipeline: RoutePipeline,
    #[cfg(feature = "middleware")]
    pub(super) cors: Option<Arc<CorsHeaders>>
}

/// Represents route path node
#[derive(Clone)]
pub(super) struct RouteEntry {
    path: Box<str>,
    node: Box<RouteNode>
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
}

/// Parameters of a route
pub(super) struct RouteParams<'route> {
    pub(super) route: &'route RouteNode,
    pub(super) params: PathArgs
}

impl RouteEntry {
    /// Creates a new [`RouteEntry`]
    #[inline]
    fn new(path: &str) -> Self {
        Self { 
            node: Box::new(RouteNode::new()),
            path: path.into()
        }
    }

    /// Compares two route entries
    #[inline(always)]
    fn cmp(&self, path: &str) -> std::cmp::Ordering {
        self.path
            .as_ref()
            .cmp(path)
    }
}

impl RouteEndpoint {
    /// Creates a new [`RouteEndpoint`]
    #[inline]
    fn new(method: Method) -> Self {
        Self {
            method,
            pipeline: RoutePipeline::new(),
            #[cfg(feature = "middleware")]
            cors: None
        }
    }
    
    /// Inserts a layer into the pipeline
    #[inline]
    fn insert(&mut self, handler: Layer) {
        self.pipeline.insert(handler);
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
        }
    }

    /// Inserts a handler to the route tree
    pub(super) fn insert(
        &mut self,
        path: &str,
        method: Method,
        handler: Layer,
    ) {
        let mut current = self;
        let path_segments = split_path(path);

        for segment in path_segments {
            if Self::is_dynamic_segment(segment) {
                current = current.insert_dynamic_node(segment);
            } else {
                current = current.insert_static_node(segment);
            }
        }

        current.insert_handler(method, handler);
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
                    name: next.path.clone(),
                    value: segment.into()
                });
                current = next.node.as_ref();
                continue;
            }

            return None;
        }
        
        (!current
            .handlers
            .as_ref()
            .is_none_or(|h| h.is_empty()))
            .then_some(RouteParams {
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

        (!current
            .handlers
            .as_ref()
            .is_none_or(|h| h.is_empty()))
            .then_some(current)
    }

    /// Returns a reference to the handler for the given method
    #[inline]
    pub(super) fn handler(&self, method: &Method) -> Option<&RouteEndpoint> {
        let handlers = self.handlers.as_ref()?;
        let i = handlers.binary_search_by(|h| h.cmp(method)).ok()?;
        Some(&handlers[i])
    }

    /// Returns a mutable reference to the handler for the given method
    #[inline]
    #[cfg(feature = "middleware")]
    pub(super) fn handler_mut(&mut self, method: &Method) -> Option<&mut RouteEndpoint> {
        let i = self.handlers.as_ref()?
            .binary_search_by(|h| h.cmp(method))
            .ok()?;

        Some(&mut self.handlers.as_mut()?[i])
    }
    
    #[cfg(feature = "middleware")]
    pub(super) fn compose(&mut self) {
        // Compose all static routes
        self.static_routes
            .iter_mut()
            .for_each(|r| r.node.compose());

        // Compose a dynamic route if present
        if let Some(route) = self.dynamic_route.as_mut() {
            route.node.compose();
        }
        
        // Compose oute endpoint pipeline if present
        if let Some(handlers) = self.handlers.as_mut() {
            handlers
                .iter_mut()
                .for_each(|r| r.pipeline.compose());
        }
    }

    /// Traverses the route tree and collects all available routes
    /// Returns a vector of tuples containing (HTTP method, route path)
    #[cfg(debug_assertions)]
    pub(super) fn collect(&self) -> super::meta::RoutesInfo {
        let mut routes = Vec::new();
        self.traverse_routes(&mut routes, String::new());
        super::meta::RoutesInfo(routes)
    }

    #[cfg(debug_assertions)]
    fn traverse_routes(
        &self, 
        routes: &mut Vec<super::meta::RouteInfo>, 
        current_path: String
    ) {
        // Traverse static routes
        for route in self.static_routes.iter() {
            let new_path = if current_path.is_empty() {
                format!("/{}", route.path)
            } else {
                format!("{current_path}/{}", route.path)
            };
            route.node.traverse_routes(routes, new_path);
        }
        
        // Traverse dynamic route (if any)
        if let Some(route) = &self.dynamic_route {
            let new_path = if current_path.is_empty() {
                format!("/{}", route.path)
            } else {
                format!("{current_path}/{}", route.path)
            };
            route.node.traverse_routes(routes, new_path);
        }

        // Record handlers for this node
        let Some(ref handlers) = self.handlers else { 
            return;
        };

        for handler in handlers.iter() {
            let route_path = if current_path.is_empty() {
                "/".to_string()
            } else {
                current_path.clone()
            };
            routes.push(super::meta::RouteInfo::new(handler.method.clone(), &route_path));
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

    #[inline(always)]
    fn insert_dynamic_node(&mut self, segment: &str) -> &mut Self {
        self
            .dynamic_route
            .get_or_insert_with(|| RouteEntry::new(segment))
            .node
            .as_mut()
    }

    #[inline(always)]
    fn insert_handler(&mut self, method: Method, handler: Layer) {
        let handlers = self
            .handlers
            .get_or_insert_with(SmallVec::new);

        let endpoint = match handlers.binary_search_by(|r| r.cmp(&method)) {
            Ok(i) => &mut handlers[i],
            Err(i) => {
                handlers.insert(i, RouteEndpoint::new(method));
                &mut handlers[i]
            }
        };
        endpoint.insert(handler);
    }

    #[inline(always)]
    fn is_dynamic_segment(segment: &str) -> bool {
        segment.starts_with(OPEN_BRACKET) &&
        segment.ends_with(CLOSE_BRACKET)
    }
}

#[inline(always)]
pub(super) fn make_allowed_str<const N: usize>(handlers: &SmallVec<[RouteEndpoint; N]>) -> String {
    if handlers.is_empty() { 
        return String::new();
    } 
    
    let mut allowed = String::with_capacity(handlers.len() * DEFAULT_DEPTH);
    let mut iter = handlers.iter().map(|h| h.method.as_str());
    if let Some(first) = iter.next() {
        allowed.push_str(first);
        for s in iter {
            allowed.push(ALLOW_METHOD_SEPARATOR);
            allowed.push_str(s);
        }
    }
    allowed
}

#[inline(always)]
fn split_path(path: &str) -> impl Iterator<Item = &str> {
    memchr_split_nonempty(PATH_SEPARATOR, path.as_bytes())
        .map(|s| std::str::from_utf8(s)
            .expect("Invalid UTF-8 sequence in path"))
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
    use hyper::Method;
    use smallvec::SmallVec;
    use crate::ok;
    use crate::http::endpoints::handlers::{Func, RouteHandler};
    use crate::http::endpoints::route::{make_allowed_str, method_order, split_path, RouteNode, DEFAULT_DEPTH};
    use super::RouteEndpoint;
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
        route.insert("/api/v1/users/{id}", Method::GET, handler.clone().into());
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
            Method::TRACE
        ];
        for i in 0..methods.len() - 1 {
            assert!(method_order(&methods[i]) < method_order(&methods[i + 1]));
        }
    }
    
    #[test]
    fn it_splits_path() {
        let path = "/a/b/c/d";
        let split = split_path(path);
        assert_eq!(
            split.collect::<Vec<_>>(),
            vec!["a", "b", "c", "d"]
        )
    }
    
    #[test]
    fn it_splits_path_with_trailing_slash() {
        let path = "/a/b/c/d/";
        let split = split_path(path);
        assert_eq!(
            split.collect::<Vec<_>>(),
            vec!["a", "b", "c", "d"]
        )
    }
    
    #[test]
    fn it_splits_path_without_leading_slash() {
        let path = "a/b/c/d";
        let split = split_path(path);
        assert_eq!(
            split.collect::<Vec<_>>(),
            vec!["a", "b", "c", "d"]
        )
    }
    
    #[test]
    fn it_makes_allowed_str() {
        let handlers: SmallVec<[RouteEndpoint; DEFAULT_DEPTH]> = smallvec::smallvec![
            RouteEndpoint::new(Method::GET),
            RouteEndpoint::new(Method::HEAD),
        ];
        
        let allowed = make_allowed_str(&handlers);
        assert_eq!(allowed, "GET,HEAD");
    }

    #[test]
    fn it_makes_empty_allowed_str_if_no_handlers() {
        let handlers: SmallVec<[RouteEndpoint; DEFAULT_DEPTH]> = smallvec::smallvec![];
        let allowed = make_allowed_str(&handlers);
        assert_eq!(allowed, "");
    }
}