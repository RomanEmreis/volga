﻿use tokio::io;
use tokio_util::sync::CancellationToken;
use futures_util::future::BoxFuture;

use std::{
    io::{Error, ErrorKind::InvalidInput},
    sync::Arc
};

use hyper::{
    header::{HeaderValue, CONTENT_LENGTH, ALLOW}, 
    body::{Body, SizeHint, Incoming}, 
    Request, 
    service::Service, 
    Method, 
    HeaderMap
};

use crate::{app::Pipeline, HttpResponse, HttpResult, HttpRequest, HttpBody, status};

use crate::app::endpoints::RouteOption;
#[cfg(feature = "middleware")]
use crate::HttpContext;

/// Represents the execution scope of the current connection
#[derive(Clone)]
pub(super) struct Scope {
    pub(super) pipeline: Arc<Pipeline>,
    pub(super) cancellation_token: CancellationToken
}

impl Service<Request<Incoming>> for Scope {
    type Response = HttpResponse;
    type Error = Error;
    type Future = BoxFuture<'static, Result<Self::Response, Self::Error>>;

    #[inline]
    fn call(&self, request: Request<Incoming>) -> Self::Future {
        Box::pin(Self::handle_request(
            HttpRequest(request), 
            self.pipeline.clone(), 
            self.cancellation_token.clone()
        ))
    }
}

impl Scope {
    pub(super) fn new(pipeline: Arc<Pipeline>) -> Self {
        Self { 
            cancellation_token: CancellationToken::new(),
            pipeline
        }
    }
    
    pub(super) async fn handle_request(
        mut request: HttpRequest, 
        pipeline: Arc<Pipeline>, 
        cancellation_token: CancellationToken
    ) -> io::Result<HttpResponse>
    {
        match pipeline.endpoints().get_endpoint(request.method(), request.uri()) {
            RouteOption::RouteNotFound => status!(404),
            RouteOption::MethodNotFound(allowed) => status!(405, [
                (ALLOW, allowed)
            ]),
            RouteOption::Ok(endpoint_context) => {
                let (handler, params) = endpoint_context.into_parts();

                let extensions = request.extensions_mut();
                extensions.insert(cancellation_token);
                extensions.insert(params);

                let request_method = request.method().clone();

                let response: HttpResult;
                #[cfg(feature = "middleware")]
                {
                    response = if pipeline.has_middleware_pipeline() {
                        let ctx = HttpContext::new(request, handler);
                        pipeline.execute(ctx).await
                    } else {
                        handler.call(request).await
                    };
                }
                #[cfg(not(feature = "middleware"))]
                {
                    response = handler.call(request).await;
                }

                match response {
                    Ok(mut response) => {
                        if request_method == Method::HEAD {
                            Self::keep_content_length(response.size_hint(), response.headers_mut());
                            *response.body_mut() = HttpBody::empty();
                        }
                        Ok(response)
                    },
                    Err(error) if error.kind() == InvalidInput => status!(400, error.to_string()),
                    Err(error) => status!(500, error.to_string())
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