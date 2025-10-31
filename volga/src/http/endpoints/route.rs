use hyper::Method;
use crate::http::endpoints::handlers::RouteHandler;
use crate::{status, HttpResult};
use smallvec::SmallVec;

#[cfg(feature = "middleware")]
use crate::middleware::{
    from_handler,
    HttpContext,
    Middlewares,
    MiddlewareFn,
    NextFn,
};

#[cfg(not(feature = "middleware"))]
use crate::http::request::HttpRequest;

const OPEN_BRACKET: char = '{';
const CLOSE_BRACKET: char = '}';
const PATH_SEPARATOR: char = '/';
const DEFAULT_DEPTH: usize = 4;

/// Route path arguments
pub(crate) type PathArgs = SmallVec<[PathArg; DEFAULT_DEPTH]>;

#[inline(always)]
pub(crate) fn empty_path_args_iter<const N: usize>() -> smallvec::IntoIter<[PathArg; N]> {
    SmallVec::<[PathArg; N]>::new().into_iter()
}

/// A single path argument
#[derive(Clone)]
pub(crate) struct PathArg {
    /// Argument name
    pub(crate) name: Box<str>,
    
    /// Argument value
    pub(crate) value: Box<str>,
}

impl PathArg {
    /// Creates a string in key=value format
    #[inline]
    pub(crate) fn query_format(&self) -> String {
        format!("{}={}", self.name, self.value)
    }
}

/// A layer of middleware or a route handler
#[derive(Clone)]
pub(crate) enum Layer {
    Handler(RouteHandler),
    #[cfg(feature = "middleware")]
    Middleware(MiddlewareFn),
}

impl From<RouteHandler> for Layer {
    #[inline]
    fn from(handler: RouteHandler) -> Self {
        Self::Handler(handler)
    }
}

#[cfg(feature = "middleware")]
impl From<MiddlewareFn> for Layer {
    #[inline]
    fn from(mw: MiddlewareFn) -> Self {
        Self::Middleware(mw)
    }
}

impl From<Layer> for RouteHandler {
    #[inline]
    fn from(layer: Layer) -> Self {
        match layer {
            Layer::Handler(handler) => handler,
            #[cfg(feature = "middleware")]
            Layer::Middleware(_) => unreachable!(),
        }
    }
}

#[cfg(feature = "middleware")]
impl From<Layer> for MiddlewareFn {
    #[inline]
    fn from(layer: Layer) -> Self {
        match layer { 
            Layer::Middleware(mw) => mw,
            Layer::Handler(handler) => from_handler(handler)
        }
    }
}

/// Route's middleware pipeline
#[derive(Clone)]
pub(crate) enum RoutePipeline {
    #[cfg(feature = "middleware")]
    Builder(Middlewares),
    #[cfg(feature = "middleware")]
    Middleware(Option<NextFn>),
    #[cfg(not(feature = "middleware"))]
    Handler(Option<RouteHandler>)
}

impl From<Layer> for RoutePipeline {
    fn from(handler: Layer) -> Self {
        #[cfg(feature = "middleware")]
        let pipeline = Self::Builder(Middlewares::from(MiddlewareFn::from(handler)));
        #[cfg(not(feature = "middleware"))]
        let pipeline = Self::Handler(Some(RouteHandler::from(handler)));
        pipeline
    }
}

impl RoutePipeline {
    /// Creates s new middleware pipeline
    pub(super) fn new() -> Self {
        #[cfg(feature = "middleware")]
        let pipeline = Self::Builder(Middlewares::new());
        #[cfg(not(feature = "middleware"))]
        let pipeline = Self::Handler(None);
        pipeline
    }
    
    /// Inserts a layer into the pipeline
    pub(super) fn insert(&mut self, layer: Layer) {
        match self {
            #[cfg(feature = "middleware")]
            Self::Builder(mx) => mx.add(layer.into()),
            #[cfg(feature = "middleware")]
            Self::Middleware(_) => (),
            #[cfg(not(feature = "middleware"))]
            Self::Handler(ref mut route_handler) => *route_handler = Some(layer.into()),
        }
    }

    /// Calls the pipeline chain
    #[cfg(feature = "middleware")]
    pub(crate) async fn call(self, ctx: HttpContext) -> HttpResult {
        match self {
            Self::Middleware(Some(next)) => {
                let next = next.clone();
                next(ctx).await
            },
            _ => status!(405)
        }
    }
    
    /// Calls the request handler
    #[cfg(not(feature = "middleware"))]
    pub(crate) async fn call(self, req: HttpRequest) -> HttpResult {
        match self { 
            Self::Handler(Some(handler)) => handler.call(req).await,
            _ => status!(405)
        }
    }

    /// Builds a middleware pipeline
    #[cfg(feature = "middleware")]
    pub(super) fn compose(&mut self) {
        let next = match self {
            Self::Middleware(_) => return,
            Self::Builder(mx) => {
                // Unlike global, in route middlewares the route handler 
                // initially locates at the beginning of the pipeline, 
                // so we need to take it to the end
                mx.pipeline.rotate_left(1);
                mx.compose()
            },
        };
        *self = Self::Middleware(next)
    }
}

/// Represents a full route's "local" middleware pipeline
/// with handler 
#[derive(Clone)]
pub(crate) struct RouteEndpoint {
    pub(crate) method: Method,
    pub(crate) pipeline: RoutePipeline
}

/// Represents route path node
#[derive(Clone)]
pub(crate) struct RouteEntry {
    path: Box<str>,
    node: Box<RouteNode>
}

/// A node in the route tree
#[derive(Clone)]
pub(crate) struct RouteNode {
    /// A list of associated endpoints for each HTTP method
    pub(crate) handlers: Option<SmallVec<[RouteEndpoint; DEFAULT_DEPTH]>>,
    
    /// List of static routes
    static_routes: SmallVec<[RouteEntry; DEFAULT_DEPTH]>,
    
    /// Dynamic route
    dynamic_route: Option<RouteEntry>,
}

/// Parameters of a route
pub(crate) struct RouteParams<'route> {
    pub(crate) route: &'route RouteNode,
    pub(crate) params: PathArgs
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
        Self { method, pipeline: RoutePipeline::new() }
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
    pub(crate) fn new() -> Self {
        Self {
            static_routes: SmallVec::new(),
            handlers: None,
            dynamic_route: None,
        }
    }

    /// Inserts a handler to the route tree
    pub(crate) fn insert(
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
    pub(crate) fn find(&self, path: &str) -> Option<RouteParams<'_>> {
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
            .map_or(true, |h| h.is_empty()))
            .then_some(RouteParams {
                route: current,
                params,
            })
    }
    
    #[cfg(feature = "middleware")]
    pub(crate) fn compose(&mut self) {
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
    pub(crate) fn collect(&self) -> super::meta::RoutesInfo {
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
fn split_path(path: &str) -> impl Iterator<Item = &str> {
    path.trim_matches(PATH_SEPARATOR)
        .split(PATH_SEPARATOR)
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
        _ => 255,
    }
}

#[cfg(test)]
mod tests {
    use hyper::Method;
    use crate::ok;
    use crate::http::endpoints::handlers::{Func, RouteHandler};
    use crate::http::endpoints::route::RouteNode;
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
}