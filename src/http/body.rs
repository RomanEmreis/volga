use bytes::{Bytes};
use futures_util::TryStreamExt;
use hyper::body::{Body, Frame, Incoming};
use tokio::fs::File;
use tokio_util::io::ReaderStream;
use serde::Serialize;
use std::io::{Error, ErrorKind};
use std::pin::Pin;
use std::task::{Context, Poll};
use pin_project_lite::pin_project;
use http_body_util::{BodyExt, Empty, Full, StreamBody};

pub type BoxBody = http_body_util::combinators::BoxBody<Bytes, Error>;
pub type UnsyncBoxBody = http_body_util::combinators::UnsyncBoxBody<Bytes, Error>;

pin_project! {
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
}

impl HttpBody {
    #[inline]
    pub(crate) fn into_inner(self) -> InnerBody {
        self.inner
    }

    #[inline]
    pub(crate) fn incoming(inner: Incoming) -> Self {
        Self { inner: InnerBody::Incoming { inner } }
    }

    #[allow(dead_code)]
    pub(crate) fn boxed<B>(inner: B) -> Self
    where 
        B: Body<Data = Bytes, Error = Error> + Send + Sync + 'static
    {
        let inner = inner.boxed();
        Self { inner: InnerBody::Boxed { inner } }
    }
    
    #[inline]
    pub fn json<T: Serialize>(content: T) -> BoxBody {
        match serde_json::to_vec(&content) {
            Ok(content) => Full::from(content)
                .map_err(|never| match never {})
                .boxed(),
            Err(e) => {
                let error_message = format!("JSON serialization error: {}", e);
                Full::from(Bytes::from(error_message))
                    .map_err(|never| match never {})
                    .boxed()
            }
        }
    }
    
    #[inline]
    pub fn full<T: Into<Bytes>>(chunk: T) -> BoxBody {
        Full::new(chunk.into())
            .map_err(|never| match never {})
            .boxed()
    }

    #[inline]
    pub fn empty() -> BoxBody {
        Empty::<Bytes>::new()
            .map_err(|never| match never {})
            .boxed()
    }
    
    #[inline]
    pub fn wrap_stream(content: File) -> BoxBody {
        let reader_stream = ReaderStream::new(content);
        let stream_body = StreamBody::new(reader_stream.map_ok(Frame::data));
        stream_body.boxed()
    }
}


