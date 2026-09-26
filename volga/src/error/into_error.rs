//! Conversion of a request handler's `Err` into an [`Error`]

use super::{BoxError, Error};
use crate::http::StatusCode;
use std::borrow::Cow;
use std::io::Error as IoError;

/// Trait for types that can be the error of a request handler's [`Result`]
///
/// A handler returning `Result<T, E>` answers `Ok` with `T` as a response. An `Err` is turned
/// into an [`Error`] through this trait and handed to the error handler: the one set with
/// [`App::map_err`](crate::App::map_err), or the default one. It is handled there as any other
/// error is, whatever `E` is.
///
/// It is the one impl an error type needs. Implementing it also gives `From<T> for Error`, so
/// `?` converts the type wherever an [`Error`] is expected, such as in a handler returning
/// [`HttpResult`](crate::HttpResult) or in middleware.
///
/// # Implemented for
/// - every error type volga converts into an [`Error`]: [`std::io::Error`], `serde_json::Error`,
///   [`Infallible`](std::convert::Infallible) and the rest
/// - [`StatusCode`]: an error with that status and its canonical reason as the message
/// - `(StatusCode, E)`: an error with that status, wrapping `E`, which is a message or any
///   error type: `(StatusCode::BAD_REQUEST, "name is required")`
/// - `String`, `&'static str`, `Cow<'static, str>`, `Box<str>`: `500` with that message
/// - `Box<dyn std::error::Error + Send + Sync>`: `500`
/// - [`Problem<E>`](crate::error::Problem) (feature `problem-details`): an error with the
///   problem's status, answering with the problem itself
///
/// It is also the error of a filter's `Result<(), E>` (feature `middleware`), which answers
/// as a handler's does, with one exception. The strings and
/// `Box<dyn std::error::Error + Send + Sync>` carry no status of their own, and answer `400`
/// from a filter instead of `500`: there, such an error is taken for the reason the request
/// is refused, as `false` is.
///
/// Integers are left out on purpose: `Err(404)` reads as a status and as an application's
/// error code alike, and `Err(StatusCode::NOT_FOUND)` says the same thing, checked at
/// compile time.
///
/// # Example
/// ```no_run
/// use volga::{App, Json, error::{Error, IntoError}, http::StatusCode};
///
/// enum ApiError {
///     NotFound(u64),
///     Unavailable(std::io::Error),
/// }
///
/// impl IntoError for ApiError {
///     fn into_error(self) -> Error {
///         match self {
///             ApiError::NotFound(id) => Error::from_parts(
///                 StatusCode::NOT_FOUND,
///                 None,
///                 format!("item {id} not found"),
///             ),
///             ApiError::Unavailable(err) => Error::server_error(err),
///         }
///     }
/// }
///
/// fn find(id: u64) -> Result<Json<u64>, ApiError> {
///     if id == 0 {
///         return Err(ApiError::NotFound(id));
///     }
///     Ok(Json(id))
/// }
///
/// # #[tokio::main]
/// # async fn main() -> std::io::Result<()> {
/// let mut app = App::new();
///
/// app.map_get("/items/{id}", find);
/// # app.run().await
/// # }
/// ```
///
/// An error answering with a body of its own attaches it with
/// [`Error::with_response`].
///
/// # `IntoError` and `From`
///
/// `From<T> for Error` comes with `IntoError`, through a blanket impl, so a type implements
/// `IntoError` and not `From`: having both is a conflict (E0119). A type with a `From` impl of
/// its own still converts with `?`, but it is not a handler's `Err` until the body of its
/// `from` moves into `into_error`. [`Error`] itself does not implement `IntoError`, since that
/// would give it a second `From<Error>`; a handler's `Result<T, Error>` is answered by an impl
/// of its own.
///
/// ```no_run
/// use volga::{HttpResult, error::{Error, IntoError}, http::StatusCode, ok};
///
/// struct Gone;
///
/// impl IntoError for Gone {
///     fn into_error(self) -> Error {
///         Error::from_parts(StatusCode::GONE, None, "gone")
///     }
/// }
///
/// fn find() -> Result<u64, Gone> {
///     Err(Gone)
/// }
///
/// // `?` converts through the `From<Gone> for Error` that `IntoError` gives
/// fn handler() -> HttpResult {
///     let id = find()?;
///     ok!("{id}")
/// }
/// ```
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be the error of a request handler's `Result`",
    label = "not an error",
    note = "the `Err` of a handler's `Result<T, E>` is turned into a `volga::error::Error` and handed to the error handler; `E` can be `Error`, `std::io::Error`, `StatusCode`, `(StatusCode, E)`, `String` or `&'static str`",
    note = "implement `volga::error::IntoError` for `{Self}`; it gives `From<{Self}> for volga::error::Error` as well, so a `from` of its own moves into `into_error`"
)]
pub trait IntoError {
    /// Converts the value into the [`Error`] the error handler receives
    fn into_error(self) -> Error;

    /// Converts the value into the [`Error`] a filter that returns it answers with
    ///
    /// The same as [`into_error`](Self::into_error), except for the errors without a status
    /// of their own - the strings and `Box<dyn std::error::Error + Send + Sync>` - which
    /// answer `400` from a filter rather than `500`. Hidden, since it exists for those alone.
    #[doc(hidden)]
    #[inline]
    fn into_filter_error(self) -> Error
    where
        Self: Sized,
    {
        self.into_error()
    }

    /// Describes the responses this error answers with in the route's OpenAPI operation
    ///
    /// Called when a route whose handler returns `Result<T, Self>` is mapped. The default
    /// describes nothing. That suits most errors: the error handler decides how they look,
    /// and the status of one is known only when it happens.
    ///
    /// Available with the `openapi` feature only, so an implementation of it has to be
    /// compiled with that feature too.
    ///
    /// # Example
    /// ```no_run
    /// use volga::{error::{Error, IntoError}, http::StatusCode, openapi::OpenApiRouteConfig};
    ///
    /// struct NotFound;
    ///
    /// impl IntoError for NotFound {
    ///     fn into_error(self) -> Error {
    ///         Error::from_parts(StatusCode::NOT_FOUND, None, "not found")
    ///     }
    ///
    ///     fn describe_openapi(config: OpenApiRouteConfig) -> OpenApiRouteConfig {
    ///         config.produces_text(404)
    ///     }
    /// }
    /// ```
    #[cfg(feature = "openapi")]
    fn describe_openapi(
        config: crate::openapi::OpenApiRouteConfig,
    ) -> crate::openapi::OpenApiRouteConfig {
        config
    }
}

/// `?` converts every [`IntoError`] into an [`Error`]
///
/// This is why an error type needs `IntoError` alone, and why volga's own error types
/// implement it rather than `From`: a `From<T> for Error` next to it would be a second impl of
/// the same conversion. It does not overlap with the reflexive `From<Error> for Error`, because
/// [`Error`] does not implement [`IntoError`].
impl<E: IntoError> From<E> for Error {
    #[inline]
    fn from(err: E) -> Self {
        err.into_error()
    }
}

impl IntoError for StatusCode {
    #[inline]
    fn into_error(self) -> Error {
        let reason = self.canonical_reason().unwrap_or("unknown status code");
        Error::from_parts(self, None, reason)
    }
}

impl<E: Into<BoxError>> IntoError for (StatusCode, E) {
    #[inline]
    fn into_error(self) -> Error {
        let (status, err) = self;
        Error::from_parts(status, None, err)
    }
}

impl IntoError for String {
    #[inline]
    fn into_error(self) -> Error {
        Error::server_error(self)
    }

    #[inline]
    fn into_filter_error(self) -> Error {
        Error::client_error(self)
    }
}

impl IntoError for &'static str {
    #[inline]
    fn into_error(self) -> Error {
        Error::server_error(self)
    }

    #[inline]
    fn into_filter_error(self) -> Error {
        Error::client_error(self)
    }
}

impl IntoError for Cow<'static, str> {
    #[inline]
    fn into_error(self) -> Error {
        Error::server_error(self)
    }

    #[inline]
    fn into_filter_error(self) -> Error {
        Error::client_error(self)
    }
}

impl IntoError for Box<str> {
    #[inline]
    fn into_error(self) -> Error {
        Error::server_error(String::from(self))
    }

    #[inline]
    fn into_filter_error(self) -> Error {
        Error::client_error(String::from(self))
    }
}

impl IntoError for BoxError {
    #[inline]
    fn into_error(self) -> Error {
        Error::server_error(self)
    }

    /// A boxed [`Error`] or [`io::Error`](IoError) keeps the status it has; any other boxed
    /// error answers `400`
    fn into_filter_error(self) -> Error {
        match self.downcast::<Error>() {
            Ok(err) => *err,
            Err(err) => match err.downcast::<IoError>() {
                Ok(err) => err.into_error(),
                Err(err) => Error::client_error(err),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::IntoError;
    use crate::error::{BoxError, Error};
    use crate::http::StatusCode;
    use std::borrow::Cow;
    use std::io::{Error as IoError, ErrorKind};

    #[test]
    fn it_passes_an_error_through_a_result() {
        use crate::http::IntoResponse;

        let err = Err::<&'static str, _>(Error::from_parts(
            StatusCode::FORBIDDEN,
            Some("/x".into()),
            "nope",
        ))
        .into_response()
        .unwrap_err();

        assert_eq!(err.status(), StatusCode::FORBIDDEN);
        assert_eq!(err.instance(), Some("/x"));
        assert_eq!(err.to_string(), "nope");
    }

    #[test]
    fn it_converts_an_io_error_by_its_kind() {
        let err = IoError::new(ErrorKind::NotFound, "gone").into_error();

        assert_eq!(err.status(), StatusCode::NOT_FOUND);
        assert_eq!(err.to_string(), "gone");
    }

    #[test]
    fn it_converts_a_status_code() {
        let err = StatusCode::NOT_FOUND.into_error();

        assert_eq!(err.status(), StatusCode::NOT_FOUND);
        assert_eq!(err.to_string(), "Not Found");
        assert_eq!(err.instance(), None);
    }

    #[test]
    fn it_converts_a_status_code_with_a_message() {
        let err = (StatusCode::BAD_REQUEST, "name is required").into_error();

        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
        assert_eq!(err.to_string(), "name is required");

        let err = (StatusCode::CONFLICT, String::from("taken")).into_error();

        assert_eq!(err.status(), StatusCode::CONFLICT);
        assert_eq!(err.to_string(), "taken");
    }

    #[test]
    fn it_converts_a_status_code_with_an_error() {
        let err = (StatusCode::BAD_GATEWAY, IoError::other("upstream")).into_error();

        assert_eq!(err.status(), StatusCode::BAD_GATEWAY);
        assert_eq!(err.to_string(), "upstream");
    }

    #[test]
    fn it_converts_strings_into_server_errors() {
        let errors = [
            String::from("boom").into_error(),
            "boom".into_error(),
            Cow::<'static, str>::Borrowed("boom").into_error(),
            Box::<str>::from("boom").into_error(),
        ];

        for err in errors {
            assert_eq!(err.status(), StatusCode::INTERNAL_SERVER_ERROR);
            assert_eq!(err.to_string(), "boom");
        }
    }

    #[test]
    fn it_converts_an_error_without_a_status_into_a_client_error_for_a_filter() {
        let errors = [
            String::from("nope").into_filter_error(),
            "nope".into_filter_error(),
            Cow::<'static, str>::Borrowed("nope").into_filter_error(),
            Box::<str>::from("nope").into_filter_error(),
            BoxError::from("nope").into_filter_error(),
        ];

        for err in errors {
            assert_eq!(err.status(), StatusCode::BAD_REQUEST);
            assert_eq!(err.to_string(), "nope");
        }
    }

    #[test]
    fn it_takes_a_volga_or_io_error_out_of_the_box_for_a_filter() {
        let boxed = BoxError::from(Error::from_parts(
            StatusCode::FORBIDDEN,
            Some("/x".into()),
            "nope",
        ));
        let err = boxed.into_filter_error();

        assert_eq!(err.status(), StatusCode::FORBIDDEN);
        assert_eq!(err.instance(), Some("/x"));
        assert!(err.into_inner().downcast::<Error>().is_err());

        let err = BoxError::from(IoError::new(ErrorKind::NotFound, "gone")).into_filter_error();

        assert_eq!(err.status(), StatusCode::NOT_FOUND);
        assert_eq!(err.to_string(), "gone");
    }

    #[test]
    fn it_converts_any_other_error_for_a_filter_as_for_a_handler() {
        let err = StatusCode::UNAUTHORIZED.into_filter_error();

        assert_eq!(err.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(err.to_string(), "Unauthorized");

        let err = (StatusCode::CONFLICT, "taken").into_filter_error();

        assert_eq!(err.status(), StatusCode::CONFLICT);

        let err = IoError::other("boom").into_filter_error();

        assert_eq!(err.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn it_converts_a_boxed_error_into_a_server_error() {
        let boxed: BoxError = IoError::other("boxed").into();
        let err = boxed.into_error();

        assert_eq!(err.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(err.to_string(), "boxed");
    }

    #[test]
    fn it_gives_a_from_impl_for_the_question_mark() {
        struct Conflict;

        impl IntoError for Conflict {
            fn into_error(self) -> Error {
                Error::from_parts(StatusCode::CONFLICT, None, "conflict")
            }
        }

        fn fails() -> Result<(), Conflict> {
            Err(Conflict)
        }

        fn propagates() -> Result<(), Error> {
            fails()?;
            Ok(())
        }

        let err = propagates().unwrap_err();

        assert_eq!(err.status(), StatusCode::CONFLICT);
        assert_eq!(err.to_string(), "conflict");
        assert_eq!(Error::from(Conflict).status(), StatusCode::CONFLICT);
    }

    #[test]
    fn it_is_implemented_for_every_error_type_volga_converts_from() {
        fn into_error<T: IntoError>() {}

        into_error::<std::convert::Infallible>();
        into_error::<IoError>();
        into_error::<serde_json::Error>();
        into_error::<serde_urlencoded::ser::Error>();
        into_error::<hyper::http::Error>();
        into_error::<std::fmt::Error>();
        into_error::<hyper::http::status::InvalidStatusCode>();
        into_error::<crate::headers::InvalidHeaderValue>();
        into_error::<crate::headers::InvalidHeaderName>();
        into_error::<crate::headers::MaxSizeReached>();
        into_error::<crate::headers::ToStrError>();
        into_error::<crate::validation::ValidationError>();
        into_error::<crate::validation::Invalid<IoError>>();

        #[cfg(feature = "di")]
        into_error::<crate::di::error::Error>();
        #[cfg(feature = "ws")]
        into_error::<tokio_tungstenite::tungstenite::Error>();
        #[cfg(feature = "oauth")]
        into_error::<crate::auth::oauth::OAuthError>();
    }

    #[test]
    fn it_converts_a_type_of_its_own() {
        struct Teapot;

        impl IntoError for Teapot {
            fn into_error(self) -> Error {
                Error::from_parts(StatusCode::IM_A_TEAPOT, None, "teapot")
            }
        }

        let err = Teapot.into_error();

        assert_eq!(err.status(), StatusCode::IM_A_TEAPOT);
        assert_eq!(err.to_string(), "teapot");
    }
}
