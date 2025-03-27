//! HTTP request utilities

use std::ops::{Deref, DerefMut};
use http_body_util::BodyDataStream;
use hyper::{
    body::Incoming,
    http::request::Parts,
    Request
};

use crate::{
    error::Error,
    headers::{FromHeaders, Header},
    HttpBody,
    UnsyncBoxBody,
    BoxBody
};

use crate::http::{
    endpoints::args::FromRequestRef, 
    request::request_body_limit::RequestBodyLimit
};

#[cfg(feature = "di")]
use crate::di::{Container, Inject};
#[cfg(feature = "di")]
use std::sync::Arc;

pub mod request_body_limit;

/// Wraps the incoming [`Request`] to enrich its functionality
pub struct HttpRequest {
    pub inner: Request<HttpBody>
}

impl Deref for HttpRequest {
    type Target = Request<HttpBody>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl DerefMut for HttpRequest {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}

impl HttpRequest {
    /// Creates a new [`HttpRequest`]
    pub fn new(request: Request<Incoming>) -> Self {
        Self { inner: request.map(HttpBody::incoming) }
    }
    
    /// Turns [`HttpRequest's`] body into limited body if it's specified
    pub fn into_limited(self, body_limit: RequestBodyLimit) -> Self {
        match body_limit {
            RequestBodyLimit::Disabled => self,
            RequestBodyLimit::Enabled(limit) => {
                let (parts, body) = self.into_parts();
                let body = HttpBody::limited(body, limit);
                Self::from_parts(parts, body)
            }
        }
    }
    
    /// Unwraps the inner request
    #[inline]
    pub fn into_inner(self) -> Request<HttpBody> {
        self.inner
    }

    /// Consumes the request and returns just the body
    #[inline]
    pub fn into_body(self) -> HttpBody {
        self.inner.into_body()
    }

    /// Consumes the request and returns the body as boxed trait object
    #[inline]
    pub fn into_boxed_body(self) -> BoxBody {
        self.inner
            .into_body()
            .into_boxed()
    }

    /// Consumes the request body into [`BodyDataStream`]
    #[inline]
    pub fn into_body_stream(self) -> BodyDataStream<HttpBody> {
        self.inner
            .into_body()
            .into_data_stream()
    }

    /// Consumes the request and returns the body as boxed trait object that is !Sync
    #[inline]
    pub fn into_boxed_unsync_body(self) -> UnsyncBoxBody {
        self.inner
            .into_body()
            .into_boxed_unsync()
    }

    /// Consumes the request and returns request head and body
    pub fn into_parts(self) -> (Parts, HttpBody) {
        self.inner.into_parts()
    }

    /// Creates a new `HttpRequest` with the given head and body
    pub fn from_parts(parts: Parts, body: HttpBody) -> Self {
        let request = Request::from_parts(parts, body);
        Self { inner: request }
    }
    
    /// Returns a reference to the DI container of the request scope
    #[inline]
    #[cfg(feature = "di")]
    pub fn container(&self) -> &Container {
        self.inner.extensions()
            .get::<Container>()
            .expect("DI Container must be provided")
    } 

    /// Resolves a service from Dependency Container as a clone, service must implement [`Clone`]
    #[inline]
    #[cfg(feature = "di")]
    pub async fn resolve<T: Inject + Clone + 'static>(&self) -> Result<T, Error> {
        self.container()
            .resolve::<T>()
            .await
    }

    /// Resolves a service from Dependency Container
    #[inline]
    #[cfg(feature = "di")]
    pub async fn resolve_shared<T: Inject + 'static>(&self) -> Result<Arc<T>, Error> {
        self.container()
            .resolve_shared::<T>()
            .await
    }
    
    /// Extracts a payload from request parts
    ///
    /// # Example
    /// ```no_run
    /// use volga::{HttpRequest, Query};
    /// use serde::Deserialize;
    ///
    /// #[derive(Deserialize)]
    /// struct Params {
    ///     id: u32,
    ///     key: String
    /// }
    ///
    /// # fn docs(req: HttpRequest) -> std::io::Result<()> {
    /// let params: Query<Params> = req.extract()?;
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub fn extract<T: FromRequestRef>(&self) -> Result<T, Error> {
        T::from_request(self)
    }

    /// Inserts the [`Header<T>`] to HTTP request headers
    #[inline]
    pub fn insert_header<T: FromHeaders>(&mut self, header: Header<T>) {
        let (name, value) = header.into_parts();
        self.headers_mut().insert(name, value);
    }
}

#[cfg(test)]
mod tests {
    use http_body_util::BodyExt;
    use crate::headers::{Header, Vary};
    use super::*;
    
    #[cfg(feature = "di")]
    use std::collections::HashMap;
    #[cfg(feature = "di")]
    use std::sync::Mutex;
    #[cfg(feature = "di")]
    use crate::di::ContainerBuilder;

    #[cfg(feature = "di")]
    #[allow(dead_code)]
    #[derive(Clone, Default)]
    struct InMemoryCache {
        inner: Arc<Mutex<HashMap<String, String>>>
    }
    
    #[test]
    fn it_inserts_header() {
        let req = Request::get("http://localhost")
            .body(HttpBody::empty())
            .unwrap();
        
        let (parts, body) = req.into_parts();
        let mut http_req = HttpRequest::from_parts(parts, body);
        let header = Header::<Vary>::from("foo");
        
        http_req.insert_header(header);
        
        assert_eq!(http_req.headers().get("vary").unwrap(), "foo");
    }
    
    #[test]
    fn it_extracts_from_request_ref() {
        let req = Request::get("http://localhost/")
            .header("vary", "foo")
            .body(HttpBody::empty())
            .unwrap();

        let (parts, body) = req.into_parts();
        let http_req = HttpRequest::from_parts(parts, body);
        
        let header = http_req.extract::<Header<Vary>>().unwrap();
        
        assert_eq!(*header, "foo");
    }
    
    #[tokio::test]
    async fn it_unwraps_body() {
        let req = Request::get("http://localhost/")
            .body(HttpBody::full("foo"))
            .unwrap();

        let (parts, body) = req.into_parts();
        let http_req = HttpRequest::from_parts(parts, body);
        
        let body = http_req
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();

        assert_eq!(String::from_utf8_lossy(&body), "foo");
    }

    #[tokio::test]
    async fn it_unwraps_inner_req() {
        let req = Request::get("http://localhost/")
            .body(HttpBody::full("foo"))
            .unwrap();

        let (parts, body) = req.into_parts();
        let http_req = HttpRequest::from_parts(parts, body);

        let body = http_req
            .into_inner()
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();

        assert_eq!(String::from_utf8_lossy(&body), "foo");
    }
    
    #[test]
    #[cfg(feature = "di")]
    #[should_panic]
    fn it_panic_if_there_is_no_di_container() {
        let req = Request::get("http://localhost/")
            .body(HttpBody::full("foo"))
            .unwrap();

        let (parts, body) = req.into_parts();
        let http_req = HttpRequest::from_parts(parts, body);
        
        _ = http_req.container();
    }

    #[tokio::test]
    #[cfg(feature = "di")]
    async fn it_resolves_from_di_container() {
        let mut container = ContainerBuilder::new();
        container.register_singleton(InMemoryCache::default());
        
        let req = Request::get("http://localhost/")
            .extension(container.build())
            .body(HttpBody::full("foo"))
            .unwrap();

        let (parts, body) = req.into_parts();
        let http_req = HttpRequest::from_parts(parts, body);

        let cache = http_req.resolve::<InMemoryCache>().await;
        
        assert!(cache.is_ok());
    }

    #[tokio::test]
    #[cfg(feature = "di")]
    async fn it_resolves_shared_from_di_container() {
        let mut container = ContainerBuilder::new();
        container.register_singleton(InMemoryCache::default());

        let req = Request::get("http://localhost/")
            .extension(container.build())
            .body(HttpBody::full("foo"))
            .unwrap();

        let (parts, body) = req.into_parts();
        let http_req = HttpRequest::from_parts(parts, body);

        let cache = http_req.resolve_shared::<InMemoryCache>().await;

        assert!(cache.is_ok());
    }
}
