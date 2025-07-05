use std::{borrow::Cow, collections::HashMap};
use hyper::Method;
use crate::http::endpoints::handlers::RouteHandler;
use crate::{status, HttpResult};

#[cfg(feature = "middleware")]
use crate::middleware::{
    HttpContext,
    Middlewares,
    MiddlewareFn,
    Next, 
    from_handler
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
    Middleware(Option<Next>),
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
}