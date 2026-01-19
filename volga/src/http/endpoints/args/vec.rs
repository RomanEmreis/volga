//! Extractor for [Vec<T>] that is basically [`Json<Vec<T>>`] and deserialized from JSON array

use serde::de::DeserializeOwned;
use pin_project_lite::pin_project;
use std::{
    task::{Context, Poll},
    marker::PhantomData,
    future::Future,
    pin::Pin
};

use crate::{
    http::endpoints::args::{FromPayload, Payload, Source},
    Json,
    error::Error
};

pin_project! {
    /// Future for `Vec<T>` extractor.
    pub struct ExtractVecFromPayloadFut<T, F> {
        #[pin]
        inner: F,
        _marker: PhantomData<T>
    }
}

impl<F, T> Future for ExtractVecFromPayloadFut<T, F>
where
    F: Future<Output = Result<Json<Vec<T>>, Error>>,
{
    type Output = Result<Vec<T>, Error>;

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        match this.inner.poll(cx) {
            Poll::Ready(Ok(json)) => Poll::Ready(Ok(json.into_inner())),
            Poll::Ready(Err(err)) => Poll::Ready(Err(err)),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<T> FromPayload for Vec<T>
where
    T: DeserializeOwned + Send
{
    type Future = ExtractVecFromPayloadFut<T, <Json<Vec<T>> as FromPayload>::Future>;

    #[inline]
    fn from_payload(payload: Payload<'_>) -> Self::Future {
        ExtractVecFromPayloadFut {
            inner: Json::<Vec<T>>::from_payload(payload),
            _marker: PhantomData
        }
    }

    #[inline]
    fn source() -> Source {
        Source::Body
    }
}

#[cfg(test)]
mod test {
    use serde::{Deserialize, Serialize};
    use crate::HttpBody;
    use super::*;

    #[derive(Debug, Serialize, Deserialize, Ord, PartialOrd, PartialEq, Eq)]
    struct User {
        age: i32,
        name: String,
    }
    
    #[tokio::test]
    async fn it_extracts_vec_of_integers() {
        let body = HttpBody::boxed(HttpBody::json([1, 2, 3]).unwrap());
        let payload = Payload::Body(body);
        let fut = <Vec<i32> as FromPayload>::from_payload(payload);
        let result = fut.await;
        assert_eq!(result.unwrap(), vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn it_extracts_vec_of_strings() {
        let body = HttpBody::boxed(HttpBody::json(["foo", "bar"]).unwrap());
        let payload = Payload::Body(body);
        let fut = <Vec<String> as FromPayload>::from_payload(payload);
        let result = fut.await;
        assert_eq!(result.unwrap(), vec!["foo".to_string(), "bar".to_string()]);
    }

    #[tokio::test]
    async fn it_extracts_vec_empty_array() {
        let body = HttpBody::boxed(HttpBody::json(Vec::<i32>::new()).unwrap());
        let payload = Payload::Body(body);
        let fut = <Vec<i32> as FromPayload>::from_payload(payload);
        let result = fut.await;
        assert_eq!(result.unwrap(), Vec::<i32>::new());
    }

    #[tokio::test]
    async fn it_extracts_vec_of_struct_array() {
        let body = HttpBody::boxed(HttpBody::json([
            User { age: 33, name: "John".into() },
            User { age: 30, name: "Jack".into() }
        ]).unwrap());
        let payload = Payload::Body(body);
        let fut = <Vec<User> as FromPayload>::from_payload(payload);
        let result = fut.await;
        assert_eq!(result.unwrap(), vec![
            User { age: 33, name: "John".into() },
            User { age: 30, name: "Jack".into() }
        ]);
    }
}