use tokio::{io, fs::File};
use bytes::Bytes;
use futures_util::TryStreamExt;
use http_body_util::{BodyExt, Empty, Full, StreamBody};
use hyper::body::Frame;
use tokio_util::io::ReaderStream;

pub type BoxBody = http_body_util::combinators::BoxBody<Bytes, io::Error>;

pub(crate) struct HttpBody;

impl HttpBody {
    #[inline]
    pub(crate) fn create(content: Bytes) -> BoxBody {
        Full::new(content)
            .map_err(|never| match never {})
            .boxed()    
    }

    #[inline]
    pub(crate) fn full<T: Into<Bytes>>(chunk: T) -> BoxBody {
        Full::new(chunk.into())
            .map_err(|never| match never {})
            .boxed()
    }

    #[inline]
    pub(crate) fn empty() -> BoxBody {
        Empty::<Bytes>::new()
            .map_err(|never| match never {})
            .boxed()
    }
    
    #[inline]
    pub(crate) fn wrap_stream(content: File) -> BoxBody {
        // Wrap to a tokio_util::io::ReaderStream
        let reader_stream = ReaderStream::new(content);
        // Convert to http_body_util::BoxBody
        let stream_body = StreamBody::new(reader_stream.map_ok(Frame::data));

        stream_body.boxed()
    }
}

