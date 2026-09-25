//! Conversion of a request handler's `Err` into an [`Error`]

use super::{BoxError, Error};
use crate::http::StatusCode;
use std::borrow::Cow;

/// Trait for types that can be the error of a request handler's [`Result`]
///
/// A handler returning `Result<T, E>` answers `Ok` with `T` as a response. An `Err` is turned
/// into an [`Error`] through this trait and handed to the error handler: the one set with
/// [`App::map_err`](crate::App::map_err), or the default one. It is handled there as any other
/// error is, whatever `E` is.
///
/// # Implemented for
/// - every type with `From<T> for Error`: [`Error`] itself, [`std::io::Error`],
///   `serde_json::Error`, [`Infallible`](std::convert::Infallible) and the rest of volga's
///   own. A type of your own gets `IntoError` this way as well, from its `From` impl.
/// - [`StatusCode`]: an error with that status and its canonical reason as the message
/// - `(StatusCode, E)`: an error with that status, wrapping `E`, which is a message or any
///   error type: `(StatusCode::BAD_REQUEST, "name is required")`
/// - `String`, `&'static str`, `Cow<'static, str>`, `Box<str>`: `500` with that message
/// - `Box<dyn std::error::Error + Send + Sync>`: `500`
/// - [`Problem<E>`](crate::error::Problem) (feature `problem-details`): an error with the
///   problem's status, answering with the problem itself
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
#[diagnostic::on_unimplemented(
    message = "`{Self}` cannot be the error of a request handler's `Result`",
    label = "not an error",
    note = "the `Err` of a handler's `Result<T, E>` is turned into a `volga::error::Error` and handed to the error handler; `E` can be `Error`, `std::io::Error`, `StatusCode`, `(StatusCode, E)`, `String` or `&'static str`",
    note = "implement `volga::error::IntoError` for `{Self}`, or `From<{Self}>` for `volga::error::Error`"
)]
pub trait IntoError {
    /// Converts the value into the [`Error`] the error handler receives
    fn into_error(self) -> Error;

    /// Describes the responses this error answers with in the route's OpenAPI operation
    ///
    /// Called when a route whose handler returns `Result<T, Self>` is mapped. The default
    /// describes nothing. That suits most errors: the error handler decides how they look,
    /// and the status of one is known only when it happens.
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

// Without `do_not_recommend`, a type that is not an error is reported as a missing
// `From<T> for Error`, with every `From` impl listed, instead of with the text above
#[diagnostic::do_not_recommend]
impl<E: Into<Error>> IntoError for E {
    #[inline]
    fn into_error(self) -> Error {
        self.into()
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
}

impl IntoError for &'static str {
    #[inline]
    fn into_error(self) -> Error {
        Error::server_error(self)
    }
}

impl IntoError for Cow<'static, str> {
    #[inline]
    fn into_error(self) -> Error {
        Error::server_error(self)
    }
}

impl IntoError for Box<str> {
    #[inline]
    fn into_error(self) -> Error {
        Error::server_error(String::from(self))
    }
}

impl IntoError for BoxError {
    #[inline]
    fn into_error(self) -> Error {
        Error::server_error(self)
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
    fn it_passes_an_error_through() {
        let err = Error::from_parts(StatusCode::FORBIDDEN, Some("/x".into()), "nope").into_error();

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
    fn it_converts_a_boxed_error_into_a_server_error() {
        let boxed: BoxError = IoError::other("boxed").into();
        let err = boxed.into_error();

        assert_eq!(err.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(err.to_string(), "boxed");
    }

    #[test]
    fn it_converts_a_type_with_a_from_impl() {
        struct Conflict;

        impl From<Conflict> for Error {
            fn from(_: Conflict) -> Self {
                Error::from_parts(StatusCode::CONFLICT, None, "conflict")
            }
        }

        let err = Conflict.into_error();

        assert_eq!(err.status(), StatusCode::CONFLICT);
        assert_eq!(err.to_string(), "conflict");
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
