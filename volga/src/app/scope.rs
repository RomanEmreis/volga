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
    app::AppEnv, 
    error::{Error, handler::call_weak_err_handler},
    headers::CACHE_CONTROL,
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

#[cfg(feature = "rate-limiting")]
use crate::rate_limiting::TrustedProxies;

const REQUEST_HEADERS_TOO_LARGE_MESSAGE: &str = "Request headers too large.";

/// Represents the execution scope of the current connection
#[derive(Clone)]
pub(crate) struct Scope {
    pub(crate) env: Weak<AppEnv>,
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
                self.env.clone(),
                self.cancellation_token.clone()
            )
            .map_ok(Into::into)
        )
    }
}

impl Scope {
    pub(crate) fn new(env: Weak<AppEnv>, peer_addr: SocketAddr) -> Self {
        Self {
            cancellation_token: CancellationToken::new(),
            peer_addr,
            env
        }
    }
    
    pub(super) async fn handle_request(
        request: Request<Incoming>,
        peer_addr: SocketAddr,
        env: Weak<AppEnv>,
        cancellation_token: CancellationToken
    ) -> HttpResult {
        let env = match env.upgrade() {
            Some(shared) => shared,
            None => {
                #[cfg(feature = "tracing")]
                tracing::error!("app instance could not be upgraded; aborting...");

                return status!(500)
            }
        };

        let method = request.method().clone();
        
        #[cfg(feature = "tracing")]
        let span = env
            .tracing_config
            .as_ref()
            .map(|_| {
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
            env.clone(), 
            cancellation_token
        ).await;
        
        finalize_response(
            method,
            response, 
            &env,
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
    env: Arc<AppEnv>,
    cancellation_token: CancellationToken) -> HttpResult {
    {
        let headers = request.headers();

        #[cfg(feature = "http1")]
        if let Limit::Limited(max_header_size) = env.max_header_size 
            && !check_max_header_size(headers, max_header_size) {
            #[cfg(feature = "tracing")]
            tracing::warn!("Request rejected due to headers exceeding configured limits");

            return status!(431, text: REQUEST_HEADERS_TOO_LARGE_MESSAGE);
        }

        #[cfg(feature = "http2")]
        if let Limit::Limited(max_header_count) = env.max_header_count 
            && !check_max_header_count(headers, max_header_count) {
            #[cfg(feature = "tracing")]
            tracing::warn!("Request rejected due to headers exceeding configured limits");

            return status!(431, text: REQUEST_HEADERS_TOO_LARGE_MESSAGE);
        }
    }
        
    #[cfg(feature = "static-files")]
    let request = {
        let mut request = request;
        request.extensions_mut().insert(env.host_env.clone());
        request
    };

    #[cfg(feature = "di")]
    let request = {
        let mut request = request;
        request.extensions_mut().insert(env.container.create_scope());
        request
    };
        
    let pipeline = &env.pipeline;
    match pipeline.endpoints().find(
        request.method(),
        request.uri(),
        #[cfg(feature = "middleware")] env.cors.is_enabled,
        #[cfg(feature = "middleware")] request.headers()
    ) {
        FindResult::RouteNotFound => pipeline.fallback(request).await,
        FindResult::MethodNotFound(allowed) => status!(405; [
            (ALLOW, allowed.as_ref())
        ]),
        FindResult::Ok(endpoint) => {
            #[cfg(feature = "middleware")]
            let (route_pipeline, params, cors) = endpoint.into_parts();
            #[cfg(not(feature = "middleware"))]
            let (route_pipeline, params) = endpoint.into_parts();

            let error_handler = pipeline.error_handler();
            let (mut parts, body) = request.into_parts();
                
            {
                let extensions = &mut parts.extensions;
                extensions.insert(ClientIp(peer_addr));
                extensions.insert(cancellation_token);
                extensions.insert(env.body_limit);
                extensions.insert(params);

                #[cfg(feature = "ws")]
                extensions.insert(error_handler.clone());
                    
                #[cfg(feature = "jwt-auth")]
                if let Some(bts) = &env.bearer_token_service {
                    extensions.insert(bts.clone());
                }

                #[cfg(any(
                    feature = "decompression-brotli",
                    feature = "decompression-gzip",
                    feature = "decompression-zstd",
                    feature = "decompression-full"
                ))]
                {
                    extensions.insert(env.decompression_limits);
                }

                #[cfg(feature = "rate-limiting")]
                {
                    if let Some(rate_limiter) = &env.rate_limiter {
                        extensions.insert(rate_limiter.clone());
                    }
                    
                    if let Some(trusted_proxies) = &env.trusted_proxies {
                        extensions.insert(TrustedProxies(trusted_proxies.clone()));
                    }
                }
            }

            let request = HttpRequest::new(Request::from_parts(parts.clone(), body))
                .into_limited(env.body_limit);
                
            #[cfg(feature = "middleware")]
            let response = if pipeline.has_middleware_pipeline() {
                let ctx = HttpContext::new(request, Some(route_pipeline), cors);
                pipeline.execute(ctx).await
            } else {
                route_pipeline.call(HttpContext::new(request, None, cors)).await
            };
            #[cfg(not(feature = "middleware"))]
            let response = route_pipeline.call(request).await;
                
            match response {
                Ok(response) => Ok(response),
                Err(err) => call_weak_err_handler(error_handler, parts, err).await,
            }
        }
    }
}

#[inline]
fn finalize_response(
    method: Method,
    response: HttpResult,
    shared: &AppEnv,
    #[cfg(feature = "tracing")] span_id: Option<Id>,
    #[cfg(feature = "tls")] host: Option<HeaderValue>
) -> HttpResult {
    response.map(|mut resp| {
        if method == Method::HEAD {
            keep_content_length(resp.size_hint(), resp.headers_mut());
            *resp.body_mut() = HttpBody::empty();
        }
        
        if let Some(hv) = &shared.cache_control {
            apply_default_cache_control(&mut resp, hv.clone());
        }

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
fn apply_default_cache_control(
    resp: &mut crate::HttpResponse, 
    header: HeaderValue,
) {
    if !resp.headers().contains_key(CACHE_CONTROL) {
        resp.headers_mut().insert(CACHE_CONTROL, header);
    }
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
