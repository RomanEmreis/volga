use tokio::sync::Mutex;
use hyper::{
    body::Incoming,
    Request
};
use crate::{app::endpoints::EndpointContext, HttpRequest, HttpResult, Results};

pub struct HttpContext {
    pub request: Mutex<Option<HttpRequest>>,
    pub(crate) endpoint_context: EndpointContext
}

impl HttpContext {
    #[inline]
    pub(crate) fn new(request: Request<Incoming>, endpoint_context: EndpointContext) -> Self {
        Self { 
            request: Mutex::new(request.into()),
            endpoint_context
        }
    }
    
    #[inline]
    pub(crate) async fn execute(&self) -> HttpResult {
        let mut request_guard = self.request.lock().await;
        if let Some(request) = request_guard.take() {
            drop(request_guard);
            self.endpoint_context.handler.call(request).await
        } else {
            Results::internal_server_error(None)
        }
    }
}