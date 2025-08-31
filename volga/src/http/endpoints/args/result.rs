//! Extractor for Result<T, E>

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
    pub struct ResultFromPayloadFuture<F> {
        #[pin]
        inner: F,
    }
}

impl<F, T> Future for ResultFromPayloadFuture<F>
where
    F: Future<Output = Result<T, Error>>,
{
    type Output = Result<Result<T, Error>, Error>;

    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        match this.inner.poll(cx) {
            Poll::Ready(Ok(value)) => Poll::Ready(Ok(Ok(value))),
            Poll::Ready(Err(err)) => Poll::Ready(Ok(Err(err))),
            Poll::Pending => Poll::Pending,
        }
    }
}

impl<T: FromPayload> FromPayload for Result<T, Error> {
    type Future = ResultFromPayloadFuture<T::Future>;

    #[inline]
    fn from_payload(payload: Payload) -> Self::Future {
        ResultFromPayloadFuture {
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
    async fn it_extracts_result_returns_ok_on_success() {
        let req = Request::get("/").body(()).unwrap();
        let (parts, _) = req.into_parts();

        let result = Result::<SuccessExtractor, Error>::from_payload(Payload::Parts(&parts)).await;

        assert!(result.is_ok());
        let inner_result = result.unwrap();
        assert!(inner_result.is_ok());
    }

    #[tokio::test]
    async fn it_extracts_result_returns_err_on_failure() {
        let req = Request::get("/").body(()).unwrap();
        let (parts, _) = req.into_parts();

        let result = Result::<FailureExtractor, Error>::from_payload(Payload::Parts(&parts)).await;

        assert!(result.is_ok());
        let inner_result = result.unwrap();
        assert!(inner_result.is_err());
    }

    #[tokio::test]
    async fn it_extracts_result_preserves_source() {
        assert_eq!(Result::<SuccessExtractor, Error>::source(), Source::Parts);
        assert_eq!(Result::<BodyExtractor, Error>::source(), Source::Body);
        assert_eq!(Result::<PathExtractor, Error>::source(), Source::Path);
    }

    #[tokio::test]
    async fn it_extracts_result_with_body_extractor() {
        let body = HttpBody::empty();

        let result = Result::<BodyExtractor, Error>::from_payload(Payload::Body(body)).await;

        assert!(result.is_ok());
        let inner_result = result.unwrap();
        assert!(inner_result.is_ok());
        assert_eq!(inner_result.unwrap().0, "body content");
    }

    #[tokio::test]
    async fn it_extracts_result_with_body_extractor_with_wrong_payload() {
        let req = Request::get("/").body(()).unwrap();
        let (parts, _) = req.into_parts();

        let result = Result::<BodyExtractor, Error>::from_payload(Payload::Parts(&parts)).await;

        assert!(result.is_ok());
        let inner_result = result.unwrap();
        assert!(inner_result.is_err());
    }

    #[tokio::test]
    async fn it_extracts_result_with_path_extractor() {
        let param = (Cow::Borrowed("id"), Cow::Borrowed("123"));

        let result = Result::<PathExtractor, Error>::from_payload(Payload::Path(&param)).await;

        assert!(result.is_ok());
        let inner_result = result.unwrap();
        assert!(inner_result.is_ok());
        assert_eq!(inner_result.unwrap().0, 123);
    }

    #[tokio::test]
    async fn it_extracts_result_with_path_extractor_returns_invalid_value() {
        let param = (Cow::Borrowed("id"), Cow::Borrowed("invalid"));

        let result = Result::<PathExtractor, Error>::from_payload(Payload::Path(&param)).await;

        assert!(result.is_ok());
        let inner_result = result.unwrap();
        assert!(inner_result.is_err());
    }

    #[tokio::test]
    async fn it_extracts_result_with_path_extractor_returns_wrong_payload() {
        let req = Request::get("/").body(()).unwrap();
        let (parts, _) = req.into_parts();

        let result = Result::<PathExtractor, Error>::from_payload(Payload::Parts(&parts)).await;

        assert!(result.is_ok());
        let inner_result = result.unwrap();
        assert!(inner_result.is_err());
    }

    #[tokio::test]
    async fn it_extracts_result_with_primitive_types() {
        // Test with i32
        let param = (Cow::Borrowed("id"), Cow::Borrowed("42"));
        let result = Result::<i32, Error>::from_payload(Payload::Path(&param)).await;
        assert!(result.is_ok());
        let inner_result = result.unwrap();
        assert!(inner_result.is_ok());
        assert_eq!(inner_result.unwrap(), 42);

        // Test with invalid i32
        let param = (Cow::Borrowed("id"), Cow::Borrowed("invalid"));
        let result = Result::<i32, Error>::from_payload(Payload::Path(&param)).await;
        assert!(result.is_ok());
        let inner_result = result.unwrap();
        assert!(inner_result.is_err());

        // Test with String
        let param = (Cow::Borrowed("name"), Cow::Borrowed("test"));
        let result = Result::<String, Error>::from_payload(Payload::Path(&param)).await;
        assert!(result.is_ok());
        let inner_result = result.unwrap();
        assert!(inner_result.is_ok());
        assert_eq!(inner_result.unwrap(), "test");
    }

    #[tokio::test]
    async fn it_extracts_result_with_nested_result() {
        // Test Result<Result<T, Error>, Error> - inner success
        let req = Request::get("/").body(()).unwrap();
        let (parts, _) = req.into_parts();

        let result = Result::<Result<SuccessExtractor, Error>, Error>::from_payload(Payload::Parts(&parts)).await;

        assert!(result.is_ok());
        let outer = result.unwrap();
        assert!(outer.is_ok());
        let inner = outer.unwrap();
        assert!(inner.is_ok());

        // Test Result<Result<T, Error>, Error> - inner failure
        let result = Result::<Result<FailureExtractor, Error>, Error>::from_payload(Payload::Parts(&parts)).await;

        assert!(result.is_ok());
        let outer = result.unwrap();
        assert!(outer.is_ok());
        let inner = outer.unwrap();
        assert!(inner.is_err());
    }

    #[test]
    fn it_extracts_result_future_poll_ready_ok() {
        use std::task::{Context, Poll};
        use std::pin::Pin;

        let inner_future = ok(SuccessExtractor);
        let mut result_future = ResultFromPayloadFuture { inner: inner_future };

        let waker = futures_util::task::noop_waker();
        let mut cx = Context::from_waker(&waker);

        let result = Pin::new(&mut result_future).poll(&mut cx);

        match result {
            Poll::Ready(Ok(Ok(_))) => {},
            _ => panic!("Expected Poll::Ready(Ok(Ok(_)))")
        }
    }

    #[test]
    fn it_extracts_result_future_poll_ready_err() {
        use std::task::{Context, Poll};
        use std::pin::Pin;

        let inner_future = err::<SuccessExtractor, Error>(Error::client_error("test"));
        let mut result_future = ResultFromPayloadFuture { inner: inner_future };

        let waker = futures_util::task::noop_waker();
        let mut cx = Context::from_waker(&waker);

        let result = Pin::new(&mut result_future).poll(&mut cx);

        match result {
            Poll::Ready(Ok(Err(_))) => {},
            _ => panic!("Expected Poll::Ready(Ok(Err(_)))")
        }
    }

    #[test]
    fn it_extracts_result_future_poll_pending() {
        use std::task::{Context, Poll};
        use std::pin::Pin;
        use futures_util::future::pending;

        let inner_future = pending::<Result<SuccessExtractor, Error>>();
        let mut result_future = ResultFromPayloadFuture { inner: inner_future };

        let waker = futures_util::task::noop_waker();
        let mut cx = Context::from_waker(&waker);

        let result = Pin::new(&mut result_future).poll(&mut cx);

        match result {
            Poll::Pending => {},
            _ => panic!("Expected Poll::Pending")
        }
    }

    #[tokio::test]
    async fn it_extracts_result_integration_with_real_extractors() {
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

        // Valid path should return Ok(Ok(value))
        let result = Result::<Path<Params>, Error>::from_payload(Payload::Parts(&parts)).await;
        assert!(result.is_ok());
        let inner_result = result.unwrap();
        assert!(inner_result.is_ok());
        assert_eq!(inner_result.unwrap().id, 123);

        // Test with missing path arguments - should return Ok(Err(error))
        let req = Request::get("/").body(()).unwrap();
        let (parts, _) = req.into_parts();

        let result = Result::<Path<Params>, Error>::from_payload(Payload::Parts(&parts)).await;
        assert!(result.is_ok());
        let inner_result = result.unwrap();
        assert!(inner_result.is_err());
    }

    #[tokio::test]
    async fn it_extracts_result_with_different_error_types() {
        // Test that the Result wrapper preserves the original Error type
        struct CustomExtractor;

        impl FromPayload for CustomExtractor {
            type Future = Ready<Result<Self, Error>>;

            fn from_payload(_: Payload) -> Self::Future {
                err(Error::server_error("Custom internal error"))
            }

            fn source() -> Source {
                Source::Parts
            }
        }

        let req = Request::get("/").body(()).unwrap();
        let (parts, _) = req.into_parts();

        let result = Result::<CustomExtractor, Error>::from_payload(Payload::Parts(&parts)).await;

        assert!(result.is_ok());
        let inner_result = result.unwrap();
        assert!(inner_result.is_err());

        // Verify the error message is preserved
        let error = inner_result.err().unwrap();
        assert!(error.to_string().contains("Custom internal error"));
    }

    #[tokio::test]
    async fn it_extracts_result_maintains_success_value() {
        struct ValueExtractor(i32);

        impl FromPayload for ValueExtractor {
            type Future = Ready<Result<Self, Error>>;

            fn from_payload(_: Payload) -> Self::Future {
                ok(ValueExtractor(42))
            }

            fn source() -> Source {
                Source::Parts
            }
        }

        let req = Request::get("/").body(()).unwrap();
        let (parts, _) = req.into_parts();

        let result = Result::<ValueExtractor, Error>::from_payload(Payload::Parts(&parts)).await;

        assert!(result.is_ok());
        let inner_result = result.unwrap();
        assert!(inner_result.is_ok());
        assert_eq!(inner_result.unwrap().0, 42);
    }
}