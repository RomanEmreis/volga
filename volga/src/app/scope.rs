use std::net::SocketAddr;
use tokio_util::sync::CancellationToken;
use futures_util::{TryFutureExt, future::BoxFuture};
use std::sync::{Arc, Weak};

use hyper::{
    header::{HeaderValue, CONTENT_LENGTH, ALLOW}, 
    body::{SizeHint, Incoming}, 
    Request,
    Response,
    service::Service, 
    Method, 
    HeaderMap
};

use crate::{
    app::AppInstance, 
    error::{Error, handler::call_weak_err_handler}, 
    http::endpoints::FindResult,
    HttpRequest, HttpBody, HttpResult, ClientIp,
    Limit,
    status
};

#[cfg(feature = "tls")]
use crate::{
    headers::{HOST, STRICT_TRANSPORT_SECURITY},
    tls::HstsHeader
};

#[cfg(feature = "tracing")]
use {
    crate::tracing::TracingConfig,
    tracing::{trace_span, Id}
};

#[cfg(feature = "middleware")]
use crate::middleware::HttpContext;

/// Represents the execution scope of the current connection
#[derive(Clone)]
pub(crate) struct Scope {
    pub(crate) shared: Weak<AppInstance>,
    pub(crate) cancellation_token: CancellationToken,
    peer_addr: SocketAddr
}

impl Service<Request<Incoming>> for Scope {
    type Response = Response<HttpBody>;
    type Error = Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    #[inline]
    fn call(&self, request: Request<Incoming>) -> Self::Future {
        Box::pin(
            Self::handle_request(
                request,
                self.peer_addr,
                self.shared.clone(),
                self.cancellation_token.clone()
            )
            .map_ok(Into::into)
        )
    }
}

impl Scope {
    pub(crate) fn new(shared: Weak<AppInstance>, peer_addr: SocketAddr) -> Self {
        Self {
            cancellation_token: CancellationToken::new(),
            peer_addr,
            shared
        }
    }
    
    pub(super) async fn handle_request(
        request: Request<Incoming>,
        peer_addr: SocketAddr,
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

        #[cfg(feature = "tracing")]
        let span = shared
            .tracing_config
            .as_ref()
            .map(|_| {
                let method = request.method();
                let uri = request.uri();
                trace_span!("request", %method, %uri)
            });

        #[cfg(feature = "tracing")]
        let _guard = span.as_ref().map(|s| s.enter());

        #[cfg(feature = "tls")]
        let host = request
            .headers()
            .get(HOST)
            .cloned();

        let response = handle_impl(
            request, 
            peer_addr, 
            shared.clone(), 
            cancellation_token
        ).await;
        
        finalize_response(
            response, 
            &shared,
            #[cfg(feature = "tracing")]
            span.as_ref().and_then(|s| s.id()),
            #[cfg(feature = "tls")]
            host
        )
    }
}

async fn handle_impl(        
    request: Request<Incoming>,
    peer_addr: SocketAddr,
    shared: Arc<AppInstance>,
    cancellation_token: CancellationToken) -> HttpResult {
    {
        let headers = request.headers();

        #[cfg(feature = "http1")]
        if let Limit::Limited(max_header_size) = shared.max_header_size 
            && !check_max_header_size(headers, max_header_size) {
            #[cfg(feature = "tracing")]
            tracing::warn!("Request rejected due to headers exceeding configured limits");

            return status!(431, "Request headers too large");
        }

        #[cfg(feature = "http2")]
        if let Limit::Limited(max_header_count) = shared.max_header_count 
            && !check_max_header_count(headers, max_header_count) {
            #[cfg(feature = "tracing")]
            tracing::warn!("Request rejected due to headers exceeding configured limits");

            return status!(431, "Request headers too large");
        }
    }
        
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
                extensions.insert(ClientIp(peer_addr));
                extensions.insert(cancellation_token);
                extensions.insert(shared.body_limit);
                extensions.insert(params);

                #[cfg(feature = "ws")]
                extensions.insert(error_handler.clone());
                    
                #[cfg(feature = "jwt-auth")]
                if let Some(bts) = &shared.bearer_token_service {
                    extensions.insert(bts.clone());
                }

                #[cfg(feature = "rate-limiting")]
                if let Some(rate_limiter) = &shared.rate_limiter {
                    extensions.insert(rate_limiter.clone());
                }
            }

            let request = HttpRequest::new(Request::from_parts(parts.clone(), body))
                .into_limited(shared.body_limit);
                
            #[cfg(feature = "middleware")]
            let response = if pipeline.has_middleware_pipeline() {
                let ctx = HttpContext::new(request, Some(route_pipeline));
                pipeline.execute(ctx).await
            } else {
                route_pipeline.call(HttpContext::new(request, None)).await
            };
            #[cfg(not(feature = "middleware"))]
            let response = route_pipeline.call(request).await;
                
            match response {
                Ok(response) if parts.method != Method::HEAD => Ok(response),
                Ok(mut response) => {
                    keep_content_length(response.size_hint(), response.headers_mut());
                    *response.body_mut() = HttpBody::empty();
                    Ok(response)
                },
                Err(err) => call_weak_err_handler(error_handler, &parts, err).await,
            }
        }
    }
}

/// Applies response-level guarantees:
/// - tracing headers
/// - security headers (HSTS)
/// Must be called for *all* responses.
#[inline]
fn finalize_response(
    response: HttpResult,
    shared: &AppInstance,
    #[cfg(feature = "tracing")]
    span_id: Option<Id>,
    #[cfg(feature = "tls")]
    host: Option<HeaderValue>
) -> HttpResult {
    response.map(|mut resp| {
        #[cfg(feature = "tracing")]
        if let Some(tracing) = &shared.tracing_config {
            apply_tracing_headers(&mut resp, tracing, span_id);
        }

        #[cfg(feature = "tls")]
        if let Some(hsts) = &shared.hsts {
            apply_hsts_headers(&mut resp, hsts, host);
        }

        resp
    })
}

#[inline]
#[cfg(feature = "tracing")]
fn apply_tracing_headers(
    resp: &mut crate::HttpResponse,
    tracing: &TracingConfig,
    span_id: Option<Id>,
) {
    if !tracing.include_header {
        return;
    }

    let value = span_id
        .map_or(0, |id| id.into_u64())
        .to_string();

    resp.headers_mut().insert(
        tracing.span_header_name,
        value.parse().expect("valid span id"),
    );
}

#[cfg(feature = "tls")]
fn apply_hsts_headers(
    resp: &mut crate::HttpResponse,
    hsts: &HstsHeader,
    host: Option<HeaderValue>,
) {
    if is_excluded(host, &hsts.exclude_hosts) {
        println!("22");
        return;
    }

    resp.headers_mut().insert(
        STRICT_TRANSPORT_SECURITY,
        hsts.value(),
    );
}

#[inline]
#[cfg(feature = "tls")]
fn is_excluded(host: Option<HeaderValue>, exclude_hosts: &[String]) -> bool {
    host.as_ref()
        .and_then(|h| h.to_str().ok())
        .map(|h| {
            let h = normalize_host(h);
            exclude_hosts.iter().any(|e| e == h)
        })
        .unwrap_or(false)
}

#[inline]
#[cfg(feature = "tls")]
fn normalize_host(host: &str) -> &str {
    let host = host.trim();

    let host = match host.rsplit_once(':') {
        Some((h, "443")) => h,
        Some((h, "80")) => h,
        _ => host,
    };

    host.trim_end_matches('.')
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

#[inline(always)]
#[cfg(feature = "http2")]
fn check_max_header_count(headers: &HeaderMap, max_header_count: usize) -> bool {
    headers.len() < max_header_count
}

#[inline(always)]
#[cfg(feature = "http1")]
fn check_max_header_size(headers: &HeaderMap, max_header_size: usize) -> bool {
    let total_size: usize = headers.iter()
        .map(|(name, value)| name.as_str().len() + value.as_bytes().len())
        .sum();

    total_size < max_header_size
}
