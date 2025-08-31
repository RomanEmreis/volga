//! Extractor for Option<T>

use pin_project_lite::pin_project;
use std::{
    task::{Context, Poll},
    future::Future,
    pin::Pin
};
use crate::{
    http::endpoints::args::{FromPayload, Payload, Source},
    error::Error
};

pin_project! {
    /// Future for `Option<T>` extractor.
    pub struct OptionFromPayloadFuture<F> {
        #[pin]
        inner: F,
    }
}

impl<F, T> Future for OptionFromPayloadFuture<F>
where
    F: Future<Output = Result<T, Error>>,
{
    type Output = Result<Option<T>, Error>;

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        match this.inner.poll(cx) {
            Poll::Ready(Ok(value)) => Poll::Ready(Ok(Some(value))),
            Poll::Ready(Err(_)) => Poll::Ready(Ok(None)),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<T: FromPayload> FromPayload for Option<T> {
    type Future = OptionFromPayloadFuture<T::Future>;

    #[inline]
    fn from_payload(payload: Payload) -> Self::Future {
        OptionFromPayloadFuture {
            inner: T::from_payload(payload),
        }
    }

    #[inline]
    fn source() -> Source {
        T::source()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HttpBody, error::Error};
    use futures_util::future::{ok, err, Ready};
    use hyper::Request;
    use std::borrow::Cow;
    use crate::http::endpoints::route::PathArguments;

    // Test extractors for testing
    struct SuccessExtractor;

    impl FromPayload for SuccessExtractor {
        type Future = Ready<Result<Self, Error>>;

        fn from_payload(_: Payload) -> Self::Future {
            ok(SuccessExtractor)
        }

        fn source() -> Source {
            Source::Parts
        }
    }

    struct FailureExtractor;

    impl FromPayload for FailureExtractor {
        type Future = Ready<Result<Self, Error>>;

        fn from_payload(_: Payload) -> Self::Future {
            err(Error::client_error("Test error"))
        }

        fn source() -> Source {
            Source::Parts
        }
    }

    struct BodyExtractor(String);

    impl FromPayload for BodyExtractor {
        type Future = Ready<Result<Self, Error>>;

        fn from_payload(payload: Payload) -> Self::Future {
            match payload {
                Payload::Body(_) => ok(BodyExtractor("body content".to_string())),
                _ => err(Error::client_error("Expected body payload"))
            }
        }

        fn source() -> Source {
            Source::Body
        }
    }

    struct PathExtractor(u32);

    impl FromPayload for PathExtractor {
        type Future = Ready<Result<Self, Error>>;

        fn from_payload(payload: Payload) -> Self::Future {
            let Payload::Path((_, value)) = payload else {
                return err(Error::client_error("Expected path payload"));
            };

            match value.parse::<u32>() {
                Ok(id) => ok(PathExtractor(id)),
                Err(_) => err(Error::client_error("Invalid path parameter"))
            }
        }

        fn source() -> Source {
            Source::Path
        }
    }

    #[tokio::test]
    async fn it_extracts_option_returns_some() {
        let req = Request::get("/").body(()).unwrap();
        let (parts, _) = req.into_parts();

        let result = Option::<SuccessExtractor>::from_payload(Payload::Parts(&parts)).await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_some());
    }

    #[tokio::test]
    async fn it_extracts_option_returns_none() {
        let req = Request::get("/").body(()).unwrap();
        let (parts, _) = req.into_parts();

        let result = Option::<FailureExtractor>::from_payload(Payload::Parts(&parts)).await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn it_extracts_option_preserves_source() {
        assert_eq!(Option::<SuccessExtractor>::source(), Source::Parts);
        assert_eq!(Option::<BodyExtractor>::source(), Source::Body);
        assert_eq!(Option::<PathExtractor>::source(), Source::Path);
    }

    #[tokio::test]
    async fn it_extracts_option_with_body_extractor() {
        let body = HttpBody::empty();

        let result = Option::<BodyExtractor>::from_payload(Payload::Body(body)).await;

        assert!(result.is_ok());
        let option_result = result.unwrap();
        assert!(option_result.is_some());
        assert_eq!(option_result.unwrap().0, "body content");
    }

    #[tokio::test]
    async fn it_extracts_option_with_body_extractor_with_wrong_payload() {
        let req = Request::get("/").body(()).unwrap();
        let (parts, _) = req.into_parts();

        let result = Option::<BodyExtractor>::from_payload(Payload::Parts(&parts)).await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn it_extracts_option_with_path_extractor() {
        let param = (Cow::Borrowed("id"), Cow::Borrowed("123"));

        let result = Option::<PathExtractor>::from_payload(Payload::Path(&param)).await;

        assert!(result.is_ok());
        let option_result = result.unwrap();
        assert!(option_result.is_some());
        assert_eq!(option_result.unwrap().0, 123);
    }

    #[tokio::test]
    async fn it_extracts_option_with_path_extractor_returns_invalid_value() {
        let param = (Cow::Borrowed("id"), Cow::Borrowed("invalid"));

        let result = Option::<PathExtractor>::from_payload(Payload::Path(&param)).await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn it_extracts_option_with_path_extractor_returns_wrong_payload() {
        let req = Request::get("/").body(()).unwrap();
        let (parts, _) = req.into_parts();

        let result = Option::<PathExtractor>::from_payload(Payload::Parts(&parts)).await;

        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[tokio::test]
    async fn it_extracts_option_with_primitive_types() {
        // Test with i32
        let param = (Cow::Borrowed("id"), Cow::Borrowed("42"));
        let result = Option::<i32>::from_payload(Payload::Path(&param)).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some(42));

        // Test with invalid i32
        let param = (Cow::Borrowed("id"), Cow::Borrowed("invalid"));
        let result = Option::<i32>::from_payload(Payload::Path(&param)).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), None);

        // Test with String
        let param = (Cow::Borrowed("name"), Cow::Borrowed("test"));
        let result = Option::<String>::from_payload(Payload::Path(&param)).await;
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), Some("test".to_string()));
    }

    #[tokio::test]
    async fn it_extracts_option_with_nested_option() {
        // Test Option<Option<T>> - inner success
        let req = Request::get("/").body(()).unwrap();
        let (parts, _) = req.into_parts();

        let result = Option::<Option<SuccessExtractor>>::from_payload(Payload::Parts(&parts)).await;

        assert!(result.is_ok());
        let outer = result.unwrap();
        assert!(outer.is_some());
        assert!(outer.unwrap().is_some());

        // Test Option<Option<T>> - inner failure
        let result = Option::<Option<FailureExtractor>>::from_payload(Payload::Parts(&parts)).await;

        assert!(result.is_ok());
        let outer = result.unwrap();
        assert!(outer.is_some());
        assert!(outer.unwrap().is_none());
    }

    #[test]
    fn it_extracts_option_future_poll_ready_ok() {
        use std::task::{Context, Poll};
        use std::pin::Pin;

        let inner_future = ok(SuccessExtractor);
        let mut option_future = OptionFromPayloadFuture { inner: inner_future };

        let waker = futures_util::task::noop_waker();
        let mut cx = Context::from_waker(&waker);

        let result = Pin::new(&mut option_future).poll(&mut cx);

        match result {
            Poll::Ready(Ok(Some(_))) => {},
            _ => panic!("Expected Poll::Ready(Ok(Some(_)))")
        }
    }

    #[test]
    fn it_extracts_option_future_poll_ready_err() {
        use std::task::{Context, Poll};
        use std::pin::Pin;

        let inner_future = err::<SuccessExtractor, Error>(Error::client_error("test"));
        let mut option_future = OptionFromPayloadFuture { inner: inner_future };

        let waker = futures_util::task::noop_waker();
        let mut cx = Context::from_waker(&waker);

        let result = Pin::new(&mut option_future).poll(&mut cx);

        match result {
            Poll::Ready(Ok(None)) => {},
            _ => panic!("Expected Poll::Ready(Ok(None))")
        }
    }

    #[tokio::test]
    async fn it_extracts_option_integration_with_real_extractors() {
        // Test with the existing Path extractor
        use crate::Path;
        use serde::Deserialize;

        #[derive(Deserialize)]
        struct Params {
            id: u32,
        }

        let args: PathArguments = vec![
            (Cow::Borrowed("id"), Cow::Borrowed("123"))
        ].into_boxed_slice();

        let req = Request::get("/")
            .extension(args)
            .body(())
            .unwrap();

        let (parts, _) = req.into_parts();

        // Valid path should return Some
        let result = Option::<Path<Params>>::from_payload(Payload::Parts(&parts)).await;
        assert!(result.is_ok());
        let option_result = result.unwrap();
        assert!(option_result.is_some());
        assert_eq!(option_result.unwrap().id, 123);

        // Test with missing path arguments - should return None
        let req = Request::get("/").body(()).unwrap();
        let (parts, _) = req.into_parts();

        let result = Option::<Path<Params>>::from_payload(Payload::Parts(&parts)).await;
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }
}