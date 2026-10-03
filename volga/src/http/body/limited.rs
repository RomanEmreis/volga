//! A request body bounded by the body limit

use crate::error::{BoxError, Error};
use bytes::{Buf, Bytes};
use hyper::{
    StatusCode,
    body::{Body, Frame, SizeHint},
};
use pin_project_lite::pin_project;

use std::{
    fmt,
    pin::Pin,
    task::{Context, Poll},
};

pin_project! {
    /// A body that yields at most `remaining` more bytes, and answers `413 Content Too Large`
    /// past that
    ///
    /// It is [`http_body_util::Limited`] but for two things:
    ///
    /// - **A body declaring more than fits is refused before a byte of it is read.** A
    ///   `Content-Length` over the limit says the body will not fit, so the first poll fails
    ///   without polling the body underneath - and a client that sent
    ///   `Expect: 100-continue` is never asked to send it. A body that declares nothing, or
    ///   less than it then sends, is refused once it has sent what fits, as before.
    /// - **Its errors are [`Error`]s already, with the status of whatever failed.** Going
    ///   over the limit is a `413`. An `Error` raised underneath - a decompression limit, for
    ///   one - keeps its own status instead of being wrapped in a `400`, and anything else
    ///   failing to read is the client's `400`.
    pub(crate) struct Limited<B> {
        #[pin]
        inner: B,
        remaining: usize,
    }
}

impl<B> Limited<B> {
    /// Bounds `inner` to `limit` bytes
    #[inline]
    pub(crate) fn new(inner: B, limit: usize) -> Self {
        Self {
            inner,
            remaining: limit,
        }
    }
}

impl<B> Body for Limited<B>
where
    B: Body<Data = Bytes>,
    B::Error: Into<BoxError>,
{
    type Data = Bytes;
    type Error = Error;

    #[inline]
    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let this = self.project();

        // What a body still has to send shrinks with every frame it sends, as `remaining`
        // does, so this holds at every poll - and on the first one it is the declared length
        if this.inner.size_hint().lower() > *this.remaining as u64 {
            return Poll::Ready(Some(Err(too_large())));
        }

        let frame = match this.inner.poll_frame(cx) {
            Poll::Pending => return Poll::Pending,
            Poll::Ready(None) => return Poll::Ready(None),
            Poll::Ready(Some(Err(err))) => return Poll::Ready(Some(Err(read_error(err)))),
            Poll::Ready(Some(Ok(frame))) => frame,
        };

        let res = match frame.data_ref().map(Buf::remaining) {
            Some(len) if len > *this.remaining => {
                *this.remaining = 0;
                Err(too_large())
            }
            Some(len) => {
                *this.remaining -= len;
                Ok(frame)
            }
            None => Ok(frame),
        };

        Poll::Ready(Some(res))
    }

    #[inline]
    fn is_end_stream(&self) -> bool {
        self.inner.is_end_stream()
    }

    #[inline]
    fn size_hint(&self) -> SizeHint {
        let Ok(remaining) = u64::try_from(self.remaining) else {
            return self.inner.size_hint();
        };

        let mut hint = self.inner.size_hint();
        if hint.lower() >= remaining {
            hint.set_exact(remaining)
        } else if let Some(max) = hint.upper() {
            hint.set_upper(remaining.min(max))
        } else {
            hint.set_upper(remaining)
        }
        hint
    }
}

/// A request body over the body limit
///
/// [`http_body_util::LengthLimitError`] cannot be built outside its crate, so this one stands
/// in for it, and reads the same.
#[derive(Debug)]
struct LengthLimitError;

impl fmt::Display for LengthLimitError {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("length limit exceeded")
    }
}

impl std::error::Error for LengthLimitError {}

/// The error a body over its limit fails with: `413 Content Too Large` (RFC 9110
/// Section 15.5.14)
#[inline]
fn too_large() -> Error {
    Error::from_parts(StatusCode::PAYLOAD_TOO_LARGE, None, LengthLimitError)
}

/// The error a body fails with when what it reads from fails
///
/// An [`Error`] is what a body volga built itself fails with - a decompressed one over its
/// own limits, say - and it keeps its status. Anything else could not be read off the
/// connection, which is the client's `400`.
#[inline]
fn read_error(err: impl Into<BoxError>) -> Error {
    match err.into().downcast::<Error>() {
        Ok(err) => *err,
        Err(err) => Error::client_error(err),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::stream;
    use http_body_util::{BodyExt, Full, StreamBody};
    use std::convert::Infallible;

    /// A body that sends `chunks` without declaring its length
    fn undeclared(
        chunks: &'static [&'static str],
    ) -> impl Body<Data = Bytes, Error = Infallible> + Unpin {
        StreamBody::new(stream::iter(
            chunks
                .iter()
                .map(|chunk| Ok(Frame::data(Bytes::from_static(chunk.as_bytes())))),
        ))
    }

    /// A body that declares `declared` bytes and fails if it is ever polled
    struct Declared {
        declared: u64,
    }

    impl Body for Declared {
        type Data = Bytes;
        type Error = Infallible;

        fn poll_frame(
            self: Pin<&mut Self>,
            _cx: &mut Context<'_>,
        ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
            panic!("a body declaring more than fits must not be read")
        }

        fn size_hint(&self) -> SizeHint {
            SizeHint::with_exact(self.declared)
        }
    }

    #[tokio::test]
    async fn it_reads_a_body_within_the_limit() {
        let body = Limited::new(Full::new(Bytes::from_static(b"hello")), 5);

        let bytes = body.collect().await.unwrap().to_bytes();

        assert_eq!(bytes, "hello");
    }

    #[tokio::test]
    async fn it_answers_413_for_a_declared_length_over_the_limit_without_reading() {
        let body = Limited::new(Declared { declared: 6 }, 5);

        let err = body.collect().await.unwrap_err();

        assert_eq!(err.status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(err.to_string(), "length limit exceeded");
    }

    #[tokio::test]
    async fn it_answers_413_once_an_undeclared_body_goes_over_the_limit() {
        let body = Limited::new(undeclared(&["abc", "def"]), 5);

        let err = body.collect().await.unwrap_err();

        assert_eq!(err.status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn it_yields_what_fits_before_refusing_an_undeclared_body() {
        let mut body = Limited::new(undeclared(&["abc", "def"]), 5);

        let first = body.frame().await.unwrap().unwrap();
        let second = body.frame().await.unwrap();

        assert_eq!(first.into_data().unwrap(), "abc");
        assert_eq!(second.unwrap_err().status(), StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[tokio::test]
    async fn it_keeps_the_status_of_an_error_raised_underneath() {
        let inner = StreamBody::new(stream::iter([Err::<Frame<Bytes>, _>(Error::from_parts(
            StatusCode::UNSUPPORTED_MEDIA_TYPE,
            None,
            "nope",
        ))]));
        let body = Limited::new(inner, 5);

        let err = body.collect().await.unwrap_err();

        assert_eq!(err.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
        assert_eq!(err.to_string(), "nope");
    }

    #[tokio::test]
    async fn it_answers_400_for_any_other_read_error() {
        let inner = StreamBody::new(stream::iter([Err::<Frame<Bytes>, _>(
            std::io::Error::other("connection reset"),
        )]));
        let body = Limited::new(inner, 5);

        let err = body.collect().await.unwrap_err();

        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn it_caps_the_size_hint_at_the_limit() {
        let fits = Limited::new(Full::new(Bytes::from_static(b"abc")), 5);
        let over = Limited::new(Declared { declared: 6 }, 5);
        let undeclared = Limited::new(undeclared(&["abc"]), 5);

        assert_eq!(fits.size_hint().exact(), Some(3));
        assert_eq!(over.size_hint().exact(), Some(5));
        assert_eq!(undeclared.size_hint().upper(), Some(5));
    }
}
