use std::{borrow::Cow, collections::HashMap};
use hyper::Method;
use crate::http::endpoints::handlers::RouteHandler;
use crate::{status, HttpResult};

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

const END_OF_ROUTE: &str = "";
const OPEN_BRACKET: char = '{';
const CLOSE_BRACKET: char = '}';
const DEFAULT_CAPACITY: usize = 8;

/// Route path arguments
pub(crate) type PathArguments = Box<[(Cow<'static, str>, Cow<'static, str>)]>;

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

/// A node in the route tree
#[derive(Clone)]
pub(crate) enum RouteNode {
    Static(HashMap<Cow<'static, str>, RouteNode>),
    Dynamic(HashMap<Cow<'static, str>, RouteNode>),
    Handler(HashMap<Method, RoutePipeline>),
}

/// Parameters of a route
pub(crate) struct RouteParams<'route> {
    pub(crate) route: &'route RouteNode,
    pub(crate) params: PathArguments
}

impl RouteNode {
    pub(crate) fn insert(
        &mut self, 
        path_segments: &[Cow<'static, str>], 
        method: Method, 
        handler: Layer
    ) {
        let mut current = self;
        for (index, segment) in path_segments.iter().enumerate() {
            let is_last = index == path_segments.len() - 1;
            let is_dynamic = Self::is_dynamic_segment(segment);

            current = match current {
                RouteNode::Static(map) | RouteNode::Dynamic(map) => {
                    let entry = map.entry(segment.clone()).or_insert_with(|| {
                        if is_dynamic {
                            RouteNode::Dynamic(HashMap::with_capacity(DEFAULT_CAPACITY))
                        } else {
                            RouteNode::Static(HashMap::with_capacity(DEFAULT_CAPACITY))
                        }
                    });

                    // Check if this segment is the last and add the handler
                    if is_last {
                        // Assumes the inserted or existing route has HashMap as associated data
                        match entry {
                            RouteNode::Dynamic(ref mut map) |
                            RouteNode::Static(ref mut map) => {
                                if let Some(endpoint) = map.get_mut(END_OF_ROUTE) { 
                                    match endpoint {
                                        RouteNode::Handler(ref mut methods) =>
                                            methods
                                                .entry(method.clone())
                                                .or_insert_with(RoutePipeline::new)
                                                .insert(handler.clone()),
                                        _ => unreachable!()
                                    };
                                } else { 
                                    let node = RouteNode::Handler(HashMap::from([
                                        (method.clone(), RoutePipeline::from(handler.clone()))
                                    ]));
                                    map.insert(END_OF_ROUTE.into(), node);
                                }
                            },
                            _ => ()
                        }
                    }
                    entry // Continue traversing or inserting into this entry
                },
                RouteNode::Handler(_) => panic!("Attempt to insert a route under a handler"),
            };
        }
    }

    pub(crate) fn find(&self, path_segments: &[Cow<'static, str>]) -> Option<RouteParams> {
        let mut current = Some(self);
        let mut params = Vec::new();
        for (index, segment) in path_segments.iter().enumerate() {
            let is_last = index == path_segments.len() - 1;

            current = match current {
                Some(RouteNode::Static(map)) | Some(RouteNode::Dynamic(map)) => {
                    // Trying direct match first
                    let direct_match = map.get(segment);

                    // If no direct match, try dynamic route resolution
                    let resolved_route = direct_match.or_else(|| {
                        map.iter()
                            .filter(|(key, _)| Self::is_dynamic_segment(key))
                            .map(|(key, route)| {
                                if let Some(param_name) = key.strip_prefix(OPEN_BRACKET).and_then(|k| k.strip_suffix(CLOSE_BRACKET)) {
                                    params.push((Cow::Owned(param_name.to_owned()), segment.clone()));
                                }
                                route
                            })
                            .next()
                    });

                    // Retrieve handler or further route if this is the last segment
                    if let Some(route) = resolved_route {
                        if is_last {
                            match route {
                                RouteNode::Dynamic(inner_map) | RouteNode::Static(inner_map) => {
                                    // Attempt to get the handler directly if no further routing is possible
                                    inner_map.get(END_OF_ROUTE).or(Some(route))
                                },
                                handler @ RouteNode::Handler(_) => Some(handler), // Direct handler return
                            }
                        } else {
                            Some(route) // Continue on non-terminal routes
                        }
                    } else {
                        None // No route resolved
                    }
                },
                _ => None,
            };
        }

        current.map(|route| RouteParams { route, params: params.into_boxed_slice() })
    }
    
    #[cfg(feature = "middleware")]
    pub(crate) fn compose(&mut self) {
        match self {
            RouteNode::Static(map) | 
            RouteNode::Dynamic(map) => map
                .values_mut()
                .for_each(|route| route.compose()),
            RouteNode::Handler(map) => map
                .values_mut()
                .for_each(|pipeline| pipeline.compose())
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
    fn traverse_routes(&self, routes: &mut Vec<super::meta::RouteInfo>, current_path: String) {
        match self {
            RouteNode::Static(map) | RouteNode::Dynamic(map) => {
                for (segment, node) in map {
                    if segment == END_OF_ROUTE {
                        // This is a handler node at the end of a route
                        node.traverse_routes(routes, current_path.clone());
                    } else {
                        // Build the path for this segment
                        let new_path = if current_path.is_empty() {
                            format!("/{segment}", )
                        } else {
                            format!("{current_path}/{segment}")
                        };
                        node.traverse_routes(routes, new_path);
                    }
                }
            }
            RouteNode::Handler(methods) => {
                // We've reached a handler node - collect all HTTP methods for this route
                for method in methods.keys() {
                    let route_path = if current_path.is_empty() {
                        "/".to_string()
                    } else {
                        current_path.clone()
                    };
                    routes.push(super::meta::RouteInfo::new(method.clone(), &route_path));
                }
            }
        }
    }

    #[inline]
    fn is_dynamic_segment(segment: &str) -> bool {
        segment.starts_with(OPEN_BRACKET) && 
        segment.ends_with(CLOSE_BRACKET)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use hyper::Method;
    use crate::ok;
    use crate::http::endpoints::handlers::{Func, RouteHandler};
    use crate::http::endpoints::route::RouteNode;
    use super::super::meta::RouteInfo;
    
    #[test]
    fn it_inserts_and_finds_route() {
        let handler = || async { ok!() };
        let handler: RouteHandler = Func::new(handler);
        
        let path = ["test".into()];
        
        let mut route = RouteNode::Static(HashMap::new());
        route.insert(&path, Method::GET, handler.into());
        
        let route_params = route.find(&path);
        
        assert!(route_params.is_some());
    }

    #[test]
    fn it_inserts_and_finds_route_with_params() {
        let handler = || async { ok!() };
        let handler: RouteHandler = Func::new(handler);

        let path = ["test".into(), "{value}".into()];

        let mut route = RouteNode::Static(HashMap::new());
        route.insert(&path, Method::GET, handler.into());

        let path = ["test".into(), "some".into()];
        
        let route_params = route.find(&path).unwrap();
        let param = route_params.params.first().unwrap();
        let (_param, val) = param;
        
        assert_eq!(val, "some");
    }

    #[test]
    fn it_collects_single_static_route() {
        let handler = || async { ok!() };
        let handler: RouteHandler = Func::new(handler);

        let path = ["users".into()];

        let mut route = RouteNode::Static(HashMap::new());
        route.insert(&path, Method::GET, handler.into());

        let routes = route.collect();

        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0], (Method::GET, "/users"));
    }

    #[test]
    fn it_collects_multiple_methods_same_route() {
        let handler = || async { ok!() };
        let handler: RouteHandler = Func::new(handler);

        let path = ["users".into()];

        let mut route = RouteNode::Static(HashMap::new());
        route.insert(&path, Method::GET, handler.clone().into());
        route.insert(&path, Method::POST, handler.into());

        let routes = route.collect();

        assert_eq!(routes.len(), 2);
        assert!(routes.contains(&RouteInfo::new(Method::GET, "/users")));
        assert!(routes.contains(&RouteInfo::new(Method::POST, "/users")));
    }

    #[test]
    fn it_collects_nested_static_routes() {
        let handler = || async { ok!() };
        let handler: RouteHandler = Func::new(handler);

        let path1 = ["users".into()];
        let path2 = ["users".into(), "profile".into()];

        let mut route = RouteNode::Static(HashMap::new());
        route.insert(&path1, Method::GET, handler.clone().into());
        route.insert(&path2, Method::GET, handler.into());

        let routes = route.collect();

        assert_eq!(routes.len(), 2);
        assert!(routes.contains(&RouteInfo::new(Method::GET, "/users")));
        assert!(routes.contains(&RouteInfo::new(Method::GET, "/users/profile")));
    }

    #[test]
    fn it_collects_dynamic_routes() {
        let handler = || async { ok!() };
        let handler: RouteHandler = Func::new(handler);

        let path = ["users".into(), "{id}".into()];

        let mut route = RouteNode::Static(HashMap::new());
        route.insert(&path, Method::GET, handler.into());

        let routes = route.collect();

        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0], (Method::GET, "/users/{id}"));
    }

    #[test]
    fn it_collects_mixed_static_and_dynamic_routes() {
        let handler = || async { ok!() };
        let handler: RouteHandler = Func::new(handler);

        let path1 = ["users".into()];
        let path2 = ["users".into(), "{id}".into()];
        let path3 = ["users".into(), "{id}".into(), "posts".into()];

        let mut route = RouteNode::Static(HashMap::new());
        route.insert(&path1, Method::GET, handler.clone().into());
        route.insert(&path2, Method::GET, handler.clone().into());
        route.insert(&path3, Method::GET, handler.into());

        let routes = route.collect();
        
        assert_eq!(routes.len(), 3);
        assert!(routes.contains(&RouteInfo::new(Method::GET, "/users")));
        assert!(routes.contains(&RouteInfo::new(Method::GET, "/users/{id}")));
        assert!(routes.contains(&RouteInfo::new(Method::GET, "/users/{id}/posts")));
    }

    #[test]
    fn it_collects_root_route() {
        let handler = || async { ok!() };
        let handler: RouteHandler = Func::new(handler);

        let path = ["".into()];

        let mut route = RouteNode::Static(HashMap::new());
        route.insert(&path, Method::GET, handler.into());

        let routes = route.collect();

        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0], (Method::GET, "/"));
    }

    #[test]
    fn it_collects_complex_route_tree() {
        let handler = || async { ok!() };
        let handler: RouteHandler = Func::new(handler);

        let mut route = RouteNode::Static(HashMap::new());

        // Add various routes
        route.insert(&["api".into(), "v1".into(), "users".into()], Method::GET, handler.clone().into());
        route.insert(&["api".into(), "v1".into(), "users".into()], Method::POST, handler.clone().into());
        route.insert(&["api".into(), "v1".into(), "users".into(), "{id}".into()], Method::GET, handler.clone().into());
        route.insert(&["api".into(), "v1".into(), "users".into(), "{id}".into()], Method::PUT, handler.clone().into());
        route.insert(&["api".into(), "v1".into(), "users".into(), "{id}".into()], Method::DELETE, handler.clone().into());
        route.insert(&["api".into(), "v1".into(), "posts".into()], Method::GET, handler.clone().into());
        route.insert(&["api".into(), "v2".into(), "users".into()], Method::GET, handler.into());

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
    fn it_handles_empty_route_tree() {
        let route = RouteNode::Static(HashMap::new());

        let routes = route.collect();

        assert_eq!(routes.len(), 0);
    }

    #[test]
    fn it_collects_routes_with_multiple_dynamic_segments() {
        let handler = || async { ok!() };
        let handler: RouteHandler = Func::new(handler);

        let path = ["users".into(), "{userId}".into(), "posts".into(), "{postId}".into(), "comments".into()];

        let mut route = RouteNode::Static(HashMap::new());
        route.insert(&path, Method::GET, handler.into());

        let routes = route.collect();

        assert_eq!(routes.len(), 1);
        assert_eq!(routes[0], (Method::GET, "/users/{userId}/posts/{postId}/comments"));
    }

    #[test]
    fn it_collects_routes_with_different_http_methods() {
        let handler = || async { ok!() };
        let handler: RouteHandler = Func::new(handler);

        let path = ["resource".into()];

        let mut route = RouteNode::Static(HashMap::new());
        route.insert(&path, Method::GET, handler.clone().into());
        route.insert(&path, Method::POST, handler.clone().into());
        route.insert(&path, Method::PUT, handler.clone().into());
        route.insert(&path, Method::DELETE, handler.clone().into());
        route.insert(&path, Method::PATCH, handler.into());

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