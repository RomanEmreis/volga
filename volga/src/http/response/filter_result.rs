//! Tools and utilities for filter and validation results.

use crate::HttpResult;
use crate::error::{BoxError, Error, IntoError};
use crate::http::IntoResponse;
use std::io::Error as IoError;
use std::ops::{Deref, DerefMut};

/// Result of filter or validation middleware.
///
/// A failed result answers with the status of its error:
/// - [`FilterResult::err`], or `false`: `400 Bad Request` with a generic message
/// - an [`Error`]: its own status, instance and attached response, as it would from a handler
/// - a [`std::io::Error`]: the status its kind maps to, as it would from a handler
///   (`NotFound` -> `404`, `PermissionDenied` -> `403`, `Other` -> `500`)
/// - any other error, a string included: `400 Bad Request` with that error as the message
#[derive(Debug)]
pub struct FilterResult(Result<(), Error>);

impl Deref for FilterResult {
    type Target = Result<(), Error>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for FilterResult {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl IntoResponse for FilterResult {
    #[inline]
    fn into_response(self) -> HttpResult {
        self.0.into_response()
    }
}

/// An `Err` is kept or converted as [`FilterResult::with_error`] does it
impl<E> From<Result<(), E>> for FilterResult
where
    E: Into<BoxError>,
{
    #[inline]
    fn from(value: Result<(), E>) -> Self {
        match value {
            Ok(()) => Self::ok(),
            Err(error) => Self::err().with_error(error),
        }
    }
}

impl From<()> for FilterResult {
    #[inline]
    fn from(_: ()) -> Self {
        Self::ok()
    }
}

impl From<bool> for FilterResult {
    #[inline]
    fn from(value: bool) -> Self {
        if value { Self::ok() } else { Self::err() }
    }
}

impl FilterResult {
    /// Creates a new, valid [`FilterResult`].
    #[inline]
    pub fn ok() -> Self {
        Self(Ok(()))
    }

    /// Creates a new, invalid [`FilterResult`].
    ///
    /// It answers `400 Bad Request` with a generic message.
    #[inline]
    pub fn err() -> Self {
        Self(Err(Error::client_error(
            "Validation: One or more request parameters are incorrect",
        )))
    }

    /// Unwraps the inner result.
    #[inline]
    pub fn into_inner(self) -> Result<(), Error> {
        self.0
    }

    /// Updates the result with the given error.
    ///
    /// An [`Error`] is kept as it is, so the filter answers with its status, its instance and
    /// the response attached with [`Error::with_response`]. A [`std::io::Error`] answers with
    /// the status its kind maps to. Any other error answers `400 Bad Request`.
    #[inline]
    pub fn with_error(mut self, error: impl Into<BoxError>) -> Self {
        self.0 = Err(Self::filter_error(error.into()));
        self
    }

    /// Takes a [`volga::Error`](Error) and an [`io::Error`](IoError) out of the box, and turns
    /// any other error into a client error
    ///
    /// Not generic over the error type, so the downcasts are compiled once.
    fn filter_error(error: BoxError) -> Error {
        match error.downcast::<Error>() {
            Ok(error) => *error,
            Err(error) => match error.downcast::<IoError>() {
                Ok(error) => error.into_error(),
                Err(error) => Error::client_error(error),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::http::StatusCode;
    use std::io::ErrorKind;

    fn forbidden() -> Error {
        Error::from_parts(StatusCode::FORBIDDEN, Some("/custom".into()), "nope")
            .with_response(crate::Json("denied"))
    }

    fn assert_forbidden(result: FilterResult) {
        let err = result.into_inner().unwrap_err();

        assert_eq!(err.status(), StatusCode::FORBIDDEN);
        assert_eq!(err.instance(), Some("/custom"));
        assert_eq!(err.to_string(), "nope");
        assert!(err.has_response());
    }

    #[test]
    fn it_creates_ok_filter_result() {
        let result = FilterResult::ok();
        assert!(result.is_ok());
    }

    #[test]
    fn it_creates_err_filter_result() {
        let result = FilterResult::err();
        assert!(result.is_err());
        if let Err(e) = result.0 {
            assert_eq!(e.status(), StatusCode::BAD_REQUEST);
            assert_eq!(
                e.to_string(),
                "Validation: One or more request parameters are incorrect"
            );
        }
    }

    #[test]
    fn it_creates_filter_result_from_unit() {
        let result: FilterResult = ().into();
        assert!(result.is_ok());
    }

    #[test]
    fn it_creates_filter_result_from_bool() {
        let result: FilterResult = true.into();
        assert!(result.is_ok());

        let result: FilterResult = false.into();
        assert!(result.is_err());
    }

    #[test]
    fn it_creates_filter_result_from_result() {
        let ok_result: Result<(), &str> = Ok(());
        let result: FilterResult = ok_result.into();
        assert!(result.is_ok());

        let err_result: Result<(), &str> = Err("test error");
        let result: FilterResult = err_result.into();
        assert!(result.is_err());
    }

    #[test]
    fn it_creates_filter_result_with_error() {
        let result = FilterResult::err().with_error("custom error");
        assert!(result.is_err());
        if let Err(e) = result.0 {
            assert!(e.to_string().contains("custom error"));
        }
    }

    #[test]
    fn it_keeps_a_volga_error_given_with_error() {
        assert_forbidden(FilterResult::err().with_error(forbidden()));
    }

    #[test]
    fn it_keeps_a_volga_error_from_a_result() {
        assert_forbidden(Err::<(), _>(forbidden()).into());
    }

    #[test]
    fn it_unwraps_a_volga_error_inside_an_io_error() {
        assert_forbidden(FilterResult::err().with_error(IoError::from(forbidden())));
    }

    #[test]
    fn it_converts_an_io_error_by_its_kind() {
        let result: FilterResult = Err::<(), _>(IoError::new(ErrorKind::NotFound, "gone")).into();
        let err = result.into_inner().unwrap_err();

        assert_eq!(err.status(), StatusCode::NOT_FOUND);
        assert_eq!(err.to_string(), "gone");

        let err = FilterResult::err()
            .with_error(IoError::other("boom"))
            .into_inner()
            .unwrap_err();

        assert_eq!(err.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(err.to_string(), "boom");
    }

    #[test]
    fn it_turns_any_other_error_into_a_client_error() {
        let results: [FilterResult; 4] = [
            Err::<(), _>("nope").into(),
            Err::<(), _>(String::from("nope")).into(),
            Err::<(), _>(std::fmt::Error).into(),
            FilterResult::err().with_error("nope"),
        ];

        for result in results {
            let err = result.into_inner().unwrap_err();

            assert_eq!(err.status(), StatusCode::BAD_REQUEST);
            assert_eq!(err.instance(), None);
            assert!(!err.has_response());
        }
    }

    #[test]
    fn it_tests_filter_result_into_inner() {
        let result = FilterResult::ok();
        let inner = result.into_inner();
        assert!(inner.is_ok());

        let result = FilterResult::err();
        let inner = result.into_inner();
        assert!(inner.is_err());
    }

    #[test]
    fn it_tests_filter_result_deref() {
        let result = FilterResult::ok();
        assert!(result.is_ok()); // Tests Deref

        let mut result = FilterResult::ok();
        *result = Err(Error::client_error("modified")); // Tests DerefMut
        assert!(result.is_err());
    }

    #[test]
    fn it_tests_filter_result_into_response() {
        let result = FilterResult::ok();
        let response = result.into_response();
        assert!(response.is_ok());

        let result = FilterResult::err();
        let response = result.into_response();
        assert!(response.is_err());
    }
}
