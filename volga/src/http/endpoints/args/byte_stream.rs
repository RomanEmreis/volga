//! Types and tools for working with byte streams

use crate::{error::Error, http::endpoints::args::{FromPayload, Source, Payload}, HttpBody};
use bytes::{Bytes, BytesMut};
use futures_util::{Stream, future::{Ready, ok}};
use http_body_util::BodyDataStream;
use pin_project_lite::pin_project;
use std::{
    task::{Context, Poll},
    borrow::Cow,
    fmt::Debug,
    pin::Pin
};

pin_project! {
    /// Wrapper type for byte streams.
    pub struct ByteStream<S> {
        #[pin]
        inner: S,
    }
}

impl<S> Debug for ByteStream<S> {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ByteStream(...)").finish()
    }
}

impl<S> ByteStream<S> {
    /// Creates a new byte stream
    #[inline]
    pub fn new(inner: S) -> Self {
        Self { inner }
    }

    /// Consumes the wrapper and returns the inner stream.
    #[inline]
    pub fn into_inner(self) -> S {
        self.inner
    }
}

impl<S, T> Stream for ByteStream<S>
where
    S: Stream<Item = T>,
    T: IntoByteResult,
{
    type Item = Result<Bytes, Error>;

    #[inline]
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        match self.project().inner.poll_next(cx) {
            Poll::Ready(Some(item)) => Poll::Ready(Some(item.into_byte_result())),
            Poll::Ready(None) => Poll::Ready(None),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl FromPayload for ByteStream<BodyDataStream<HttpBody>> {
    type Future = Ready<Result<Self, Error>>;
    
    const SOURCE: Source = Source::Body;

    #[inline]
    fn from_payload(payload: Payload<'_>) -> Self::Future {
        let Payload::Body(body) = payload else { unreachable!() };
        ok(Self::new(body.into_data_stream()))
    }
}

/// A helper trait for types that are suitable for byte stream
pub trait IntoByteResult {
    /// Converts a type into a bytes
    fn into_byte_result(self) -> Result<Bytes, Error>;
}

impl<T, E> IntoByteResult for Result<T, E>
where
    T: Into<Bytes>,
    E: Into<Error>,
{
    #[inline]
    fn into_byte_result(self) -> Result<Bytes, Error> {
        self.map(Into::into).map_err(Into::into)
    }
}

macro_rules! impl_into_byte_result {
    { $($ty:ty),* $(,)? } => {
        $(impl IntoByteResult for $ty {
            #[inline]
            fn into_byte_result(self) -> Result<Bytes, Error> {
                Ok(Bytes::from(self))
            }
        })*
    };
}

macro_rules! impl_into_byte_result_with {
    ( $( $ty:ty => $body:expr ),* $(,)? ) => {
        $(
            impl IntoByteResult for $ty {
                #[inline]
                fn into_byte_result(self) -> Result<Bytes, Error> {
                    Ok(($body)(self))
                }
            }
        )*
    };
}

impl_into_byte_result! {
    String, Box<[u8]>, Vec<u8>, BytesMut, Bytes
}

impl_into_byte_result_with!(
    Box<str>  => |b: Box<str>| Bytes::from(b.into_string()),
    &'static [u8] => |b: &'static [u8]| Bytes::from_static(b),
    &'static str => |b: &'static str| Bytes::from_static(b.as_bytes()),
    Cow<'_, str> => |b: Cow<'_, str>| Bytes::copy_from_slice(b.as_bytes()),
);

/// Creates an asynchronous stream
#[macro_export]
macro_rules! byte_stream {
    {$($tt:tt)*} => {{
        $crate::ByteStream::new(
            $crate::__async_stream::stream! { $($tt)* }
        )
    }};
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures_util::{pin_mut, StreamExt};

    #[tokio::test]
    async fn it_creates_byte_stream() {
        let stream = byte_stream! {
            yield "hi!";
            yield "hi!";
            yield "hi!";
        };

        pin_mut!(stream);

        let bytes = stream.next().await.unwrap().unwrap();
        assert_eq!(String::from_utf8_lossy(&bytes), "hi!");

        let bytes = stream.next().await.unwrap().unwrap();
        assert_eq!(String::from_utf8_lossy(&bytes), "hi!");

        let bytes = stream.next().await.unwrap().unwrap();
        assert_eq!(String::from_utf8_lossy(&bytes), "hi!");

        assert!(stream.next().await.is_none());
    }

    #[tokio::test]
    async fn it_creates_byte_stream_with_loop() {
        let stream = byte_stream! {
            loop {
                yield "hi!".as_bytes();
            }
        };

        pin_mut!(stream);

        let bytes = stream.next().await.unwrap().unwrap();
        assert_eq!(String::from_utf8_lossy(&bytes), "hi!");

        let bytes = stream.next().await.unwrap().unwrap();
        assert_eq!(String::from_utf8_lossy(&bytes), "hi!");

        let bytes = stream.next().await.unwrap().unwrap();
        assert_eq!(String::from_utf8_lossy(&bytes), "hi!");

    }

    #[tokio::test]
    async fn it_creates_byte_stream_of_strings() {
        let stream = ByteStream::new(futures_util::stream::iter([
            String::from("hi!"), 
            String::from("hi!")
        ]));
        
        pin_mut!(stream);
        
        let bytes = stream.next().await.unwrap().unwrap();
        assert_eq!(String::from_utf8_lossy(&bytes), "hi!");
        
        let bytes = stream.next().await.unwrap().unwrap();
        assert_eq!(String::from_utf8_lossy(&bytes), "hi!");
    }

    #[tokio::test]
    async fn it_creates_byte_stream_of_box_str() {
        let stream = ByteStream::new(futures_util::stream::iter([
            String::from("hi!").into_boxed_str(),
            String::from("hi!").into_boxed_str(),
        ]));

        pin_mut!(stream);

        let bytes = stream.next().await.unwrap().unwrap();
        assert_eq!(String::from_utf8_lossy(&bytes), "hi!");

        let bytes = stream.next().await.unwrap().unwrap();
        assert_eq!(String::from_utf8_lossy(&bytes), "hi!");
    }

    #[tokio::test]
    async fn it_creates_byte_stream_of_box_u8() {
        let stream = ByteStream::new(futures_util::stream::iter([
            String::from("hi!").into_boxed_str().into_boxed_bytes(),
            String::from("hi!").into_boxed_str().into_boxed_bytes(),
        ]));

        pin_mut!(stream);

        let bytes = stream.next().await.unwrap().unwrap();
        assert_eq!(String::from_utf8_lossy(&bytes), "hi!");

        let bytes = stream.next().await.unwrap().unwrap();
        assert_eq!(String::from_utf8_lossy(&bytes), "hi!");
    }

    #[tokio::test]
    async fn it_creates_byte_stream_of_cow() {
        let stream = ByteStream::new(futures_util::stream::iter([
            Cow::Borrowed("hi!"),
            Cow::Borrowed("hi!"),
        ]));

        pin_mut!(stream);

        let bytes = stream.next().await.unwrap().unwrap();
        assert_eq!(String::from_utf8_lossy(&bytes), "hi!");

        let bytes = stream.next().await.unwrap().unwrap();
        assert_eq!(String::from_utf8_lossy(&bytes), "hi!");
    }

    #[tokio::test]
    async fn it_creates_byte_stream_of_bytes() {
        let stream = ByteStream::new(futures_util::stream::iter([
            Bytes::from_static(b"hi!"),
            Bytes::from_static(b"hi!"),
        ]));

        pin_mut!(stream);

        let bytes = stream.next().await.unwrap().unwrap();
        assert_eq!(String::from_utf8_lossy(&bytes), "hi!");

        let bytes = stream.next().await.unwrap().unwrap();
        assert_eq!(String::from_utf8_lossy(&bytes), "hi!");
    }

    #[tokio::test]
    async fn it_creates_byte_stream_of_bytes_mut() {
        let stream = ByteStream::new(futures_util::stream::iter([
            BytesMut::from(Bytes::from_static(b"hi!")),
            BytesMut::from(Bytes::from_static(b"hi!")),
        ]));

        pin_mut!(stream);

        let bytes = stream.next().await.unwrap().unwrap();
        assert_eq!(String::from_utf8_lossy(&bytes), "hi!");

        let bytes = stream.next().await.unwrap().unwrap();
        assert_eq!(String::from_utf8_lossy(&bytes), "hi!");
    }

    #[tokio::test]
    async fn it_creates_byte_stream_from_payload() {
        let body = HttpBody::full("Hello, World!");
        
        let stream = ByteStream::from_payload(Payload::Body(body)).await.unwrap();
        pin_mut!(stream);
        
        let bytes = stream.next().await.unwrap().unwrap();
        assert_eq!(String::from_utf8_lossy(&bytes), "Hello, World!");
    }
}