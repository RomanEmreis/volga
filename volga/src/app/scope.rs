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
    http::endpoints::FindResult,
    HttpResponse, HttpRequest, HttpBody, HttpResult,
    status
};

#[cfg(feature = "middleware")]
use crate::middleware::HttpContext;

#[cfg(any(feature = "tls", feature = "tracing"))]
use std::sync::Arc;

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
        
        #[cfg(feature = "static-files")]
        let request = {
            let mut request = request;
            request.extensions_mut().insert(shared.host_env.clone());
            request
        };

        #[cfg(feature = "di")]
        let request = {
            let mut request = request;
            request.extensions_mut().insert(shared.container.create_scope());
            request
        };
        
        let pipeline = &shared.pipeline;
        match pipeline.endpoints().find(request.method(), request.uri()) {
            FindResult::RouteNotFound => pipeline.fallback(request).await,
            FindResult::MethodNotFound(allowed) => status!(405, [
                (ALLOW, allowed)
            ]),
            FindResult::Ok(endpoint) => {
                let (route_pipeline, params) = endpoint.into_parts();
                let error_handler = pipeline.error_handler();

                let (mut parts, body) = request.into_parts();
                {
                    let extensions = &mut parts.extensions;
                    extensions.insert(cancellation_token);
                    extensions.insert(shared.body_limit);
                    
                    #[cfg(feature = "jwt-auth")]
                    if let Some(bts) = &shared.bearer_token_service {
                        extensions.insert(bts.clone());
                    } 
                }

                let mut request = HttpRequest::new(Request::from_parts(parts.clone(), body))
                    .into_limited(shared.body_limit);
                
                #[cfg(any(feature = "tls", feature = "tracing"))]
                let parts = Arc::new(parts);

                {
                    let extensions = request.extensions_mut();
                    extensions.insert(params);
                    extensions.insert(error_handler.clone());

                    #[cfg(any(feature = "tls", feature = "tracing"))]
                    extensions.insert(parts.clone());
                }
                
                #[cfg(feature = "middleware")]
                let response = if pipeline.has_middleware_pipeline() {
                    let ctx = HttpContext::with_pipeline(request, route_pipeline);
                    pipeline.execute(ctx).await
                } else {
                    route_pipeline.call(HttpContext::slim(request)).await
                };
                #[cfg(not(feature = "middleware"))]
                let response = route_pipeline.call(request).await;
                
                match response {
                    Err(err) => call_weak_err_handler(error_handler, &parts, err).await,
                    Ok(response) if parts.method != Method::HEAD => Ok(response),
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