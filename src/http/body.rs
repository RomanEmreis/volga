use bytes::{Bytes};
use futures_util::TryStreamExt;
use hyper::body::{Body, Frame, Incoming, SizeHint};
use http_body_util::{BodyExt, Empty, Full, StreamBody};
use pin_project_lite::pin_project;
use serde::Serialize;
use tokio_util::io::ReaderStream;
use tokio::fs::File;

use std::{
    io::{Error, ErrorKind},
    task::{Context, Poll},
    pin::Pin,
};

pub type BoxBody = http_body_util::combinators::BoxBody<Bytes, Error>;
pub type UnsyncBoxBody = http_body_util::combinators::UnsyncBoxBody<Bytes, Error>;

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
        Incoming {
            #[pin]
            inner: Incoming
        },
        Boxed {
            #[pin]
            inner: BoxBody
        },
    }   
}

impl Body for HttpBody {
    type Data = Bytes;
    type Error = Error;

    #[inline]
    fn poll_frame(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        match self.project().inner.project() {
            InnerBodyProj::Incoming { inner } => inner.poll_frame(cx)
                .map_err(|e| Error::new(ErrorKind::InvalidInput, e)),
            InnerBodyProj::Boxed  { inner } => inner.poll_frame(cx),
        }
    }

    #[inline]
    fn is_end_stream(&self) -> bool {
        match &self.inner {
            InnerBody::Incoming { inner } => inner.is_end_stream(),
            InnerBody::Boxed  { inner } => inner.is_end_stream(),
        }
    }
    
    #[inline]
    fn size_hint(&self) -> SizeHint {
        match &self.inner {
            InnerBody::Incoming { inner } => inner.size_hint(),
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

    /// Create a new [`HttpBody`] from incoming request stream
    #[inline]
    pub(crate) fn incoming(inner: Incoming) -> Self {
        Self { inner: InnerBody::Incoming { inner } }
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

    /// Consumes the [`HttpBody`] and returns the body as boxed trait object
    #[inline]
    pub fn into_boxed(self) -> BoxBody {
        match self.inner {
            InnerBody::Boxed { inner } => inner,
            InnerBody::Incoming { inner } => inner
                .map_err(|e| Error::new(ErrorKind::InvalidInput, e))
                .boxed(),
        }
    }

    /// Consumes the [`HttpBody`] and returns the body as boxed trait object that is !Sync
    #[inline]
    pub fn into_boxed_unsync(self) -> UnsyncBoxBody {
        self.boxed_unsync()
    }
    
    /// Creates a new [`HttpBody`] from JSON object
    #[inline]
    pub fn json<T: Serialize>(content: T) -> HttpBody {
        let inner = match serde_json::to_vec(&content) {
            Ok(content) => Full::from(content)
                .map_err(|never| match never {})
                .boxed(),
            Err(e) => {
                let error_message = format!("JSON serialization error: {}", e);
                Full::from(error_message)
                    .map_err(|never| match never {})
                    .boxed()
            }
        };
        Self { inner: InnerBody::Boxed { inner } }
    }

    /// Creates a new [`HttpBody`] from object that is convertable to byte array
    #[inline]
    pub fn full<T: Into<Bytes>>(chunk: T) -> HttpBody {
        let inner = Full::new(chunk.into())
            .map_err(|never| match never {})
            .boxed();
        Self { inner: InnerBody::Boxed { inner } }
    }

    /// Creates an empty [`HttpBody`]
    #[inline]
    pub fn empty() -> HttpBody {
        let inner = Empty::<Bytes>::new()
            .map_err(|never| match never {})
            .boxed();
        Self { inner: InnerBody::Boxed { inner } }
    }

    /// Creates a new [`HttpBody`] from [`File`] stream
    #[inline]
    pub fn wrap_stream(content: File) -> HttpBody {
        let reader_stream = ReaderStream::new(content);
        let stream_body = StreamBody::new(reader_stream.map_ok(Frame::data));
        Self { inner: InnerBody::Boxed { inner: stream_body.boxed() } }
    }
}


