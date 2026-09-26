//! Tools and utilities for filter and validation results.

use crate::HttpResult;
use crate::error::{Error, IntoError};
use crate::http::IntoResponse;
use std::ops::{Deref, DerefMut};

/// Result of filter or validation middleware.
///
/// A failed result answers with the status of its error:
/// - [`FilterResult::err`], or `false`: `400 Bad Request` with a generic message
/// - an [`Error`]: its own status, instance and attached response, as it would from a handler
/// - any other type implementing [`IntoError`]: the status it converts with, as it would from
///   a handler - `StatusCode`, `(StatusCode, E)`, a [`std::io::Error`] by its kind, a
///   `ValidationError` by its own status
/// - a string or a `Box<dyn std::error::Error + Send + Sync>`: `400 Bad Request` with that
///   message, where a handler answers `500`. Such an error has no status of its own, and a
///   filter's error is taken for the reason the request is refused. A boxed [`Error`] or
///   [`std::io::Error`] still keeps its status.
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

/// An [`Error`] is kept as it is
impl From<Result<(), Error>> for FilterResult {
    #[inline]
    fn from(value: Result<(), Error>) -> Self {
        Self(value)
    }
}

/// Any other error is converted through [`IntoError`], as a handler's is, except that a string
/// or a `Box<dyn std::error::Error + Send + Sync>` answers `400`
///
/// It does not overlap with the impl for [`Error`], since [`Error`] does not implement
/// [`IntoError`].
impl<E: IntoError> From<Result<(), E>> for FilterResult {
    #[inline]
    fn from(value: Result<(), E>) -> Self {
        Self(value.map_err(IntoError::into_filter_error))
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

    /// Replaces the result with the given error.
    ///
    /// It takes what a filter's `Err` takes, and answers as `Err(error)` would: an [`Error`]
    /// as it is, with its status, its instance and the response attached with
    /// [`Error::with_response`], and any other type through [`IntoError`] - a string or a
    /// `Box<dyn std::error::Error + Send + Sync>` with `400 Bad Request`.
    #[inline]
    pub fn with_error<E>(self, error: E) -> Self
    where
        Self: From<Result<(), E>>,
    {
        Self::from(Err(error))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::BoxError;
    use crate::http::StatusCode;
    use std::borrow::Cow;
    use std::io::{Error as IoError, ErrorKind};

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

    /// The status and the message a failed result answers with
    fn answer(result: FilterResult) -> (StatusCode, String) {
        let err = result.into_inner().unwrap_err();
        (err.status(), err.to_string())
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
    fn it_answers_an_error_without_a_status_of_its_own_with_a_client_error() {
        let results: [FilterResult; 7] = [
            Err::<(), _>("nope").into(),
            Err::<(), _>(String::from("nope")).into(),
            Err::<(), _>(Cow::<'static, str>::Borrowed("nope")).into(),
            Err::<(), _>(Box::<str>::from("nope")).into(),
            Err::<(), _>(BoxError::from("nope")).into(),
            FilterResult::err().with_error("nope"),
            FilterResult::err().with_error(BoxError::from("nope")),
        ];

        for result in results {
            let err = result.into_inner().unwrap_err();

            assert_eq!(err.status(), StatusCode::BAD_REQUEST);
            assert_eq!(err.to_string(), "nope");
            assert_eq!(err.instance(), None);
            assert!(!err.has_response());
        }
    }

    #[test]
    fn it_keeps_the_status_an_error_type_converts_with() {
        struct Teapot;

        impl IntoError for Teapot {
            fn into_error(self) -> Error {
                Error::from_parts(StatusCode::IM_A_TEAPOT, None, "teapot")
            }
        }

        fn invalid() -> crate::validation::ValidationError {
            crate::validation::ValidationError::message("bad")
                .with_status(StatusCode::UNPROCESSABLE_ENTITY)
        }

        let cases: [(FilterResult, FilterResult, StatusCode, &str); 5] = [
            (
                Err::<(), _>(StatusCode::UNAUTHORIZED).into(),
                FilterResult::err().with_error(StatusCode::UNAUTHORIZED),
                StatusCode::UNAUTHORIZED,
                "Unauthorized",
            ),
            (
                Err::<(), _>((StatusCode::CONFLICT, "taken")).into(),
                FilterResult::err().with_error((StatusCode::CONFLICT, "taken")),
                StatusCode::CONFLICT,
                "taken",
            ),
            (
                Err::<(), _>(invalid()).into(),
                FilterResult::err().with_error(invalid()),
                StatusCode::UNPROCESSABLE_ENTITY,
                "bad",
            ),
            (
                Err::<(), _>(Teapot).into(),
                FilterResult::err().with_error(Teapot),
                StatusCode::IM_A_TEAPOT,
                "teapot",
            ),
            (
                Err::<(), _>(IoError::new(ErrorKind::PermissionDenied, "denied")).into(),
                FilterResult::err().with_error(IoError::new(ErrorKind::PermissionDenied, "denied")),
                StatusCode::FORBIDDEN,
                "denied",
            ),
        ];

        for (from_err, with_error, status, message) in cases {
            assert_eq!(answer(from_err), (status, message.to_owned()));
            assert_eq!(answer(with_error), (status, message.to_owned()));
        }
    }

    #[test]
    fn it_keeps_the_status_of_a_boxed_volga_or_io_error() {
        assert_forbidden(Err::<(), _>(BoxError::from(forbidden())).into());
        assert_forbidden(FilterResult::err().with_error(BoxError::from(forbidden())));

        let boxed = BoxError::from(IoError::new(ErrorKind::NotFound, "gone"));

        assert_eq!(
            answer(Err::<(), _>(boxed).into()),
            (StatusCode::NOT_FOUND, "gone".to_owned())
        );
    }

    #[test]
    #[cfg(feature = "oauth")]
    fn it_keeps_the_status_of_an_oauth_error() {
        use crate::auth::oauth::{OAuthError, OAuthErrorCode};

        let invalid_token = Err::<(), _>(OAuthError::new(OAuthErrorCode::InvalidToken));
        let insufficient_scope = OAuthError::new(OAuthErrorCode::InsufficientScope);

        assert_eq!(answer(invalid_token.into()).0, StatusCode::UNAUTHORIZED);
        assert_eq!(
            answer(FilterResult::err().with_error(insufficient_scope)).0,
            StatusCode::FORBIDDEN
        );
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
