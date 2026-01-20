//! HTTP Body utilities

use bytes::Bytes;
use hyper::body::Frame;
use pin_project_lite::pin_project;
use serde::Serialize;
use tokio_util::io::ReaderStream;
use tokio::fs::File;
use crate::error::{BoxError, Error};
use futures_util::{TryStream, TryStreamExt};

use http_body_util::{
    BodyExt,
    Empty, 
    Full, 
    StreamBody, 
    Limited, 
    BodyDataStream
};

use std::{
    borrow::Cow,
    task::{Context, Poll},
    pin::Pin,
};

pub use hyper::body::{Body, Incoming, SizeHint};

/// A boxed body
pub type BoxBody = http_body_util::combinators::BoxBody<Bytes, Error>;

/// A boxed body that is !Sync
pub type UnsyncBoxBody = http_body_util::combinators::UnsyncBoxBody<Bytes, Error>;

mod into_body;

pin_project! {
    /// Represents a response/request body
    pub struct HttpBody {
        #[pin]
        inner: InnerBody
    }
}

pin_project! {
    #[project = InnerBodyProj]
    pub(crate) enum InnerBody {
        Empty {
            #[pin]
            inner: Empty<Bytes>
        },
        Full {
            #[pin]
            inner: Full<Bytes>
        },
        Incoming {
            #[pin]
            inner: Incoming
        },
        Limited {
            #[pin]
            inner: Limited<Incoming>
        },
        Boxed {
            #[pin]
            inner: BoxBody
        },
        BoxedLimited {
            #[pin]
            inner: Limited<BoxBody>
        },
    }   
}

impl Body for HttpBody {
    type Data = Bytes;
    type Error = Error;

    #[inline]
    fn poll_frame(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        match self.project().inner.project() {
            InnerBodyProj::Empty { inner } => inner.poll_frame(cx).map_err(Error::client_error),
            InnerBodyProj::Full { inner } => inner.poll_frame(cx).map_err(Error::client_error),
            InnerBodyProj::Incoming { inner } => inner.poll_frame(cx).map_err(Error::client_error),
            InnerBodyProj::Limited { inner } => inner.poll_frame(cx).map_err(Error::client_error),
            InnerBodyProj::BoxedLimited  { inner } => inner.poll_frame(cx).map_err(Error::client_error),
            InnerBodyProj::Boxed  { inner } => inner.poll_frame(cx),
        }
    }

    #[inline]
    fn is_end_stream(&self) -> bool {
        match &self.inner {
            InnerBody::Empty { inner } => inner.is_end_stream(),
            InnerBody::Full { inner } => inner.is_end_stream(),
            InnerBody::Incoming { inner } => inner.is_end_stream(),
            InnerBody::Limited { inner } => inner.is_end_stream(),
            InnerBody::BoxedLimited  { inner } => inner.is_end_stream(),
            InnerBody::Boxed  { inner } => inner.is_end_stream(),
        }
    }
    
    #[inline]
    fn size_hint(&self) -> SizeHint {
        match &self.inner {
            InnerBody::Empty { inner } => inner.size_hint(),
            InnerBody::Full { inner } => inner.size_hint(),
            InnerBody::Incoming { inner } => inner.size_hint(),
            InnerBody::Limited { inner } => inner.size_hint(),
            InnerBody::BoxedLimited { inner } => inner.size_hint(),
            InnerBody::Boxed  { inner } => inner.size_hint(),
        }
    }
}

impl HttpBody {
    /// Creates a new [`HttpBody`]
    #[inline]
    pub fn new(inner: BoxBody) -> Self {
        Self { inner: InnerBody::Boxed { inner } }
    }

    /// Create a new [`HttpBody`] from the incoming request stream
    #[inline]
    pub(crate) fn incoming(inner: Incoming) -> Self {
        Self { inner: InnerBody::Incoming { inner } }
    }

    /// Create a new limited [`HttpBody`] from the incoming request stream
    #[inline]
    pub(crate) fn limited(inner: HttpBody, limit: usize) -> Self {
        match inner.inner {
            InnerBody::Empty { inner } => Self {
                inner: InnerBody::Empty { inner }
            },
            InnerBody::Full { inner } => Self {
                inner: InnerBody::Full { inner }
            },
            InnerBody::Limited { inner } => Self { 
                inner: InnerBody::Limited { inner }
            },
            InnerBody::BoxedLimited { inner } => Self { 
                inner: InnerBody::BoxedLimited { inner }
            },
            InnerBody::Boxed { inner } => Self { 
                inner: InnerBody::BoxedLimited { inner: Limited::new(inner, limit) }
            },
            InnerBody::Incoming { inner } => Self { 
                inner: InnerBody::Limited { inner: Limited::new(inner, limit) }
            }
        }
    }

    /// Wraps the `inner` into [`HttpBody`] as boxed trait object
    #[allow(dead_code)]
    pub(crate) fn boxed<B>(inner: B) -> Self
    where 
        B: Body<Data = Bytes, Error = Error> + Send + Sync + 'static
    {
        let inner = inner.boxed();
        Self { inner: InnerBody::Boxed { inner } }
    }

    /// Consumes the [`HttpBody`] and returns the body as a boxed trait object
    #[inline]
    pub fn into_boxed(self) -> BoxBody {
        match self.inner {
            InnerBody::Boxed { inner } => inner,
            InnerBody::Empty { inner } => inner
                .map_err(Error::client_error)
                .boxed(),
            InnerBody::Full { inner } => inner
                .map_err(Error::client_error)
                .boxed(),
            InnerBody::BoxedLimited { inner } => inner
                .map_err(Error::client_error)
                .boxed(),
            InnerBody::Limited { inner } => inner
                .map_err(Error::client_error)
                .boxed(),
            InnerBody::Incoming { inner } => inner
                .map_err(Error::client_error)
                .boxed(),
        }
    }

    /// Consumes this [`HttpBody`] into [`BodyDataStream`]
    #[inline]
    pub fn into_data_stream(self) -> BodyDataStream<HttpBody> {
        BodyExt::into_data_stream(self)
    }

    /// Consumes the [`HttpBody`] and returns the body as a boxed trait object that is !Sync.
    #[inline]
    pub fn into_boxed_unsync(self) -> UnsyncBoxBody {
        self.boxed_unsync()
    }

    /// Creates a new [`HttpBody`] from any string object.
    /// There is no allocating or copying.
    #[inline]
    pub fn text<S>(s: S) -> Self
    where
        S: Into<Cow<'static, str>>,
    {
        match s.into() {
            Cow::Borrowed(st) => Self::from_static_text(st),
            Cow::Owned(owned) => Self::full(owned)
        }
    }

    /// Creates a new [`HttpBody`] from a static slice of bytes.
    /// There is no allocating or copying.
    #[inline(always)]
    pub fn from_static(s: &'static [u8]) -> Self {
        Self::full(Bytes::from_static(s))
    }

    /// Creates a new [`HttpBody`] from a static str.
    /// There is no allocating or copying.
    #[inline(always)]
    pub fn from_static_text(s: &'static str) -> Self {
        Self::from_static(s.as_bytes())
    }

    /// Creates a new [`HttpBody`] from `&str` object 
    /// by copying it without `String` or `Box<str>` allocation.
    #[inline(always)]
    pub fn text_ref(s: &str) -> Self {
        Self::from_slice(s.as_bytes())
    }

    /// Creates a new [`HttpBody`] from a slice of bytes
    /// by copying it without `Vec<u8>` or `Box<[u8]>` allocation.
    #[inline(always)]
    pub fn from_slice(s: &[u8]) -> Self {
        Self::full(Bytes::copy_from_slice(s))
    }

    /// Creates a new [`HttpBody`] from JSON object
    #[inline]
    pub fn json<T: Serialize>(content: T) -> Result<HttpBody, Error> {
        let content = serde_json::to_vec(&content)?;
        Ok(Self { inner: InnerBody::Full { inner: Full::from(content) } })
    }

    /// Creates a new [`HttpBody`] from a Form Data object
    #[inline]
    pub fn form<T: Serialize>(content: T) -> Result<HttpBody, Error> {
        let content = serde_urlencoded::to_string(&content)?;
        Ok(Self { inner: InnerBody::Full { inner: Full::from(content) } })
    }

    /// Creates a new [`HttpBody`] from an object that is convertable to a byte array
    #[inline]
    pub fn full<T: Into<Bytes>>(chunk: T) -> HttpBody {
        Self { inner: InnerBody::Full { inner: Full::new(chunk.into()) } }
    }

    /// Creates an empty [`HttpBody`]
    #[inline]
    pub fn empty() -> HttpBody {
        Self { inner: InnerBody::Empty { inner: Empty::<Bytes>::new() } }
    }

    /// Creates a new [`HttpBody`] from [`File`] stream
    #[inline]
    pub fn file(content: File) -> HttpBody {
        let reader_stream = ReaderStream::new(content);
        Self::stream(reader_stream)
    }

    /// Creates a new [`HttpBody`] from stream
    #[inline]
    pub fn stream<S>(stream: S) -> HttpBody
    where 
        S: TryStream + Send + Sync + 'static,
        S::Ok: Into<Bytes>,
        S::Error: Into<BoxError>
    {
        let stream_body = StreamBody::new(stream
            .map_err(Error::client_error)
            .map_ok(|msg| Frame::data(msg.into())));
        Self { inner: InnerBody::Boxed { inner: stream_body.boxed() } }
    }
}

#[cfg(test)]
mod tests {
    use http_body_util::BodyExt;
    use hyper::body::Body;
    use serde::{Serialize, Serializer};
    use crate::HttpBody;
    use std::borrow::Cow;
    
    struct FailStruct;
    
    impl Serialize for FailStruct {
        fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            Err(serde::ser::Error::custom("oops..."))
        }
    }
    
    #[tokio::test]
    async fn it_returns_err_if_body_limit_exceeded() {
        let body = HttpBody::boxed(HttpBody::full("Hello, World!").into_boxed());
        let body = HttpBody::limited(body, 5);
        
        let collected = body.collect().await;
        
        assert!(collected.is_err());
    }

    #[tokio::test]
    async fn it_returns_ok_if_body_within_limit() {
        let body = HttpBody::full("Hello, World!");
        let body = HttpBody::limited(body, 100);

        let collected = body.collect().await;

        assert!(collected.is_ok());
    }

    #[tokio::test]
    async fn it_returns_error_body_if_unable_to_serialize_json() {
        let content =  FailStruct;
        let body = HttpBody::json(content);

        assert!(body.is_err());
    }

    #[tokio::test]
    async fn it_returns_error_body_if_unable_to_serialize_form() {
        let content =  FailStruct;
        let body = HttpBody::form(content);

        assert!(body.is_err());
    }

    #[tokio::test]
    async fn it_returns_empty_body() {
        let body = HttpBody::empty();

        let collected = body.collect().await;
        assert!(collected.is_ok());
        
        let size = collected.unwrap().size_hint();
        assert_eq!(size.lower(), 0);
        assert_eq!(size.upper(), None)
    }

    #[tokio::test]
    async fn it_works_with_static_str() {
        let body = HttpBody::from_static_text("Hello, World!");
        
        let collected = body.collect().await;
        
        assert_eq!(String::from_utf8(collected.unwrap().to_bytes().into()).unwrap(), "Hello, World!");
    }

    #[tokio::test]
    async fn it_works_with_static_bytes() {
        let body = HttpBody::from_static(b"Hello, World!");
        
        let collected = body.collect().await;
        
        assert_eq!(String::from_utf8(collected.unwrap().to_bytes().into()).unwrap(), "Hello, World!");
    }

    #[tokio::test]
    async fn it_works_with_string() {
        let body = HttpBody::text(String::from("Hello, World!"));
        
        let collected = body.collect().await;
        
        assert_eq!(String::from_utf8(collected.unwrap().to_bytes().into()).unwrap(), "Hello, World!");
    }

    #[tokio::test]
    async fn it_works_with_static_str_to_text() {
        let body = HttpBody::text("Hello, World!");
        
        let collected = body.collect().await;
        
        assert_eq!(String::from_utf8(collected.unwrap().to_bytes().into()).unwrap(), "Hello, World!");
    }

    #[tokio::test]
    async fn it_works_with_cow() {
        let body = HttpBody::text(Cow::<'static, str>::Borrowed("Hello, World!"));
        
        let collected = body.collect().await;
        
        assert_eq!(String::from_utf8(collected.unwrap().to_bytes().into()).unwrap(), "Hello, World!");
    }

    #[tokio::test]
    async fn it_works_with_str() {
        let string = String::from("Hello, World!");
        let body = HttpBody::text_ref(string.as_str());
        
        let collected = body.collect().await;
        
        assert_eq!(String::from_utf8(collected.unwrap().to_bytes().into()).unwrap(), "Hello, World!");
    }

    #[tokio::test]
    async fn it_works_with_slice() {
        let string = String::from("Hello, World!");
        let body = HttpBody::from_slice(string.as_bytes());
        
        let collected = body.collect().await;
        
        assert_eq!(String::from_utf8(collected.unwrap().to_bytes().into()).unwrap(), "Hello, World!");
    }
}