﻿use hyper::{
    body::Incoming,
    http::request::Parts,
    Request
};

use std::{
    io::Error,
    ops::{Deref, DerefMut}
};

use crate::{
    headers::{FromHeaders, Header},
    HttpBody,
    UnsyncBoxBody,
    BoxBody
};

#[cfg(feature = "di")]
use crate::di::{Container, Inject};
use crate::http::endpoints::args::FromRequestRef;

/// Wraps the incoming [`Request`] to enrich its functionality
pub struct HttpRequest {
    pub inner: Request<HttpBody>,
    #[cfg(feature = "di")]
    pub(crate) container: Container
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
    #[cfg(not(feature = "di"))]
    pub fn new(request: Request<Incoming>) -> Self {
        Self { inner: request.map(HttpBody::incoming) }
    }

    #[cfg(feature = "di")]
    pub fn new(request: Request<Incoming>, container: Container) -> Self {
        Self { inner: request.map(HttpBody::incoming), container }
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

    /// Consumes the request and returns the body as boxed trait object that is !Sync
    #[inline]
    pub fn into_boxed_unsync_body(self) -> UnsyncBoxBody {
        self.inner
            .into_body()
            .into_boxed_unsync()
    }

    /// Consumes the request and returns request head and body
    #[cfg(not(feature = "di"))]
    pub fn into_parts(self) -> (Parts, HttpBody) {
        self.inner.into_parts()
    }

    /// Creates a new `HttpRequest` with the given head and body
    #[cfg(not(feature = "di"))]
    pub fn from_parts(parts: Parts, body: HttpBody) -> Self {
        let request = Request::from_parts(parts, body);
        Self { inner: request }
    }

    /// Consumes the request and returns request head, body and scoped DI container
    #[cfg(feature = "di")]
    pub fn into_parts(self) -> (Parts, HttpBody, Container) {
        let (parts, body) = self.inner.into_parts();
        (parts, body, self.container)
    }
    
    /// Creates a new `HttpRequest` with the given head, body and scoped DI container
    #[cfg(feature = "di")]
    pub fn from_parts(parts: Parts, body: HttpBody, container: Container) -> Self {
        let request = Request::from_parts(parts, body);
        Self { inner: request, container }
    }

    /// Resolves a service from Dependency Container
    #[inline]
    #[cfg(feature = "di")]
    pub async fn resolve<T: Inject + 'static>(&mut self) -> Result<T, Error> {
        self.container.resolve::<T>().await
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
