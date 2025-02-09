use tokio_util::sync::CancellationToken;
use futures_util::future::BoxFuture;
use std::sync::Weak;

use hyper::{
    header::{HeaderValue, CONTENT_LENGTH, ALLOW}, 
    body::{Body, SizeHint, Incoming}, 
    Request, 
    service::Service, 
    Method, 
    HeaderMap
};

use crate::{
    app::AppInstance, 
    error::{Error, handler::call_weak_err_handler}, 
    http::endpoints::RouteOption,
    HttpResponse, HttpRequest, HttpBody, HttpResult,
    status
};

#[cfg(feature = "middleware")]
use crate::middleware::HttpContext;

/// Represents the execution scope of the current connection
#[derive(Clone)]
pub(crate) struct Scope {
    pub(crate) shared: Weak<AppInstance>,
    pub(crate) cancellation_token: CancellationToken
}

impl Service<Request<Incoming>> for Scope {
    type Response = HttpResponse;
    type Error = Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    #[inline]
    fn call(&self, request: Request<Incoming>) -> Self::Future {
        Box::pin(Self::handle_request(
            request, 
            self.shared.clone(),
            self.cancellation_token.clone()
        ))
    }
}

impl Scope {
    pub(crate) fn new(shared: Weak<AppInstance>) -> Self {
        Self {
            cancellation_token: CancellationToken::new(),
            shared
        }
    }
    
    pub(super) async fn handle_request(
        request: Request<Incoming>, 
        shared: Weak<AppInstance>,
        cancellation_token: CancellationToken
    ) -> HttpResult {
        let shared = match shared.upgrade() {
            Some(shared) => shared,
            None => {
                #[cfg(feature = "tracing")]
                tracing::warn!("app instance could not be upgraded; aborting...");
                return status!(500)
            }
        };
        
        let pipeline = &shared.pipeline;
        match pipeline.endpoints().get_endpoint(request.method(), request.uri()) {
            RouteOption::RouteNotFound => pipeline.fallback(request).await,
            RouteOption::MethodNotFound(allowed) => status!(405, [
                (ALLOW, allowed)
            ]),
            RouteOption::Ok(endpoint_context) => {
                let (handler, params) = endpoint_context.into_parts();
                
                #[cfg(feature = "di")]
                let mut request = HttpRequest::new(request, shared.container.create_scope())
                    .into_limited(shared.body_limit);
                
                #[cfg(not(feature = "di"))]
                let mut request = HttpRequest::new(request).into_limited(shared.body_limit);
                
                let extensions = request.extensions_mut();
                extensions.insert(cancellation_token);
                extensions.insert(params);
                extensions.insert(shared.body_limit);
                
                let request_method = request.method().clone();
                let uri = request.uri().clone();
                let error_handler = pipeline.error_handler();
                
                #[cfg(feature = "middleware")]
                let response = if pipeline.has_middleware_pipeline() {
                    let ctx = HttpContext::new(request, handler, error_handler.clone());
                    pipeline.execute(ctx).await
                } else {
                    handler.call(request).await
                };
                #[cfg(not(feature = "middleware"))]
                let response = handler.call(request).await;
                
                match response {
                    Err(err) => call_weak_err_handler(error_handler, &uri, err).await,
                    Ok(response) if request_method != Method::HEAD => Ok(response),
                    Ok(mut response) => {
                        Self::keep_content_length(response.size_hint(), response.headers_mut());
                        *response.body_mut() = HttpBody::empty();
                        Ok(response)
                    }
                }
            }
        }
    }
    
    fn keep_content_length(size_hint: SizeHint, headers: &mut HeaderMap) {
        if headers.contains_key(CONTENT_LENGTH) { 
            return;
        }
        
        if let Some(size) = size_hint.exact() { 
            let content_length = if size == 0 { 
                HeaderValue::from_static("0")
            } else {
                let mut buffer = itoa::Buffer::new();
                HeaderValue::from_str(buffer.format(size)).unwrap()
            };
            headers.insert(CONTENT_LENGTH, content_length);
        } 
    }
}