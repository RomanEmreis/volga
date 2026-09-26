//! Error Handling tools

use hyper::http::status::InvalidStatusCode;

use std::{
    convert::Infallible,
    error::Error as StdError,
    fmt,
    io::{Error as IoError, ErrorKind},
    sync::{Mutex, PoisonError},
};

use super::{
    App, HttpResponse,
    http::{FromRequestParts, GenericHandler, IntoResponse, MapErr, StatusCode},
};

pub use self::{
    fallback::{FallbackFunc, FallbackHandler},
    handler::{ErrorFunc, ErrorHandler},
    into_error::IntoError,
};

#[cfg(feature = "problem-details")]
pub use self::problem::{Problem, ProblemDetails};

pub mod fallback;
pub mod handler;
mod into_error;
#[cfg(feature = "problem-details")]
pub mod problem;

pub(crate) type BoxError = Box<dyn StdError + Send + Sync>;

/// Generic error
pub struct Error {
    /// HTTP status code
    pub(crate) status: StatusCode,

    /// Inner error object
    pub(crate) inner: BoxError,

    /// The instance and the attached response, allocated only once either is set
    ///
    /// Most errors carry neither until the error handler names the instance, so keeping them
    /// out of line holds an `Error` - and with it every `Result<T, Error>`, an extractor's
    /// included - at a status code and three words.
    pub(crate) extras: Option<Box<ErrorExtras>>,
}

/// The parts of an [`Error`] most errors leave empty
#[derive(Debug, Default)]
pub(crate) struct ErrorExtras {
    /// An instance where the error happened
    instance: Option<String>,

    /// A response the error answers with, attached by [`Error::with_response`]
    response: Option<AttachedResponse>,
}

impl ErrorExtras {
    /// Boxes an instance, leaving an error without one without extras
    #[inline]
    fn of_instance(instance: Option<String>) -> Option<Box<Self>> {
        instance.map(|instance| {
            Box::new(Self {
                instance: Some(instance),
                response: None,
            })
        })
    }
}

/// The response an [`Error`] answers with in place of the one its error handler would build
///
/// The `Mutex` is there for `Sync` alone: [`HttpBody`](crate::HttpBody) is `Send` but not
/// `Sync`, while an [`Error`] has to be both, since it travels as a [`BoxError`] and inside an
/// [`io::Error`](IoError). The response is only ever reached by value, so the lock is never
/// taken. It is boxed apart from the instance, so an error with an instance alone - which is
/// every error the error handler sees - allocates no room for a response.
pub(crate) struct AttachedResponse(Box<Mutex<HttpResponse>>);

impl AttachedResponse {
    #[inline]
    fn new(response: HttpResponse) -> Self {
        Self(Box::new(Mutex::new(response)))
    }

    #[inline]
    fn into_inner(self) -> HttpResponse {
        // Nothing locks it, so nothing can have poisoned it
        self.0.into_inner().unwrap_or_else(PoisonError::into_inner)
    }
}

impl fmt::Debug for AttachedResponse {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // The body is not `Debug`, and the status is the error's own
        f.debug_struct("AttachedResponse").finish_non_exhaustive()
    }
}

impl fmt::Debug for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Flat, as if the extras were fields of their own
        let response = self
            .extras
            .as_ref()
            .and_then(|extras| extras.response.as_ref());

        f.debug_struct("Error")
            .field("status", &self.status)
            .field("instance", &self.instance())
            .field("inner", &self.inner)
            .field("response", &response)
            .finish()
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.inner.fmt(f)
    }
}

impl StdError for Error {
    fn source(&self) -> Option<&(dyn StdError + 'static)> {
        Some(self.inner.as_ref())
    }
}

impl IntoError for Infallible {
    #[inline]
    fn into_error(self) -> Error {
        match self {}
    }
}

impl IntoError for serde_json::Error {
    #[inline]
    fn into_error(self) -> Error {
        Error::from_parts(StatusCode::BAD_REQUEST, None, self)
    }
}

impl IntoError for serde_urlencoded::ser::Error {
    #[inline]
    fn into_error(self) -> Error {
        Error::from_parts(StatusCode::BAD_REQUEST, None, self)
    }
}

impl IntoError for IoError {
    #[inline]
    fn into_error(self) -> Error {
        let kind = self.kind();

        if kind == ErrorKind::Other {
            if let Some(inner) = self.into_inner() {
                return match inner.downcast::<Error>() {
                    Ok(volga) => *volga,
                    Err(inner) => Error::from_io_error_fallback(IoError::new(kind, inner)),
                };
            }

            return Error::from_io_error_fallback(IoError::new(kind, "io error (Other)"));
        }

        Error::from_io_error_fallback(self)
    }
}

impl IntoError for hyper::http::Error {
    #[inline]
    fn into_error(self) -> Error {
        Error::from_parts(StatusCode::INTERNAL_SERVER_ERROR, None, self)
    }
}

impl From<Error> for IoError {
    #[inline]
    fn from(err: Error) -> Self {
        Self::other(err)
    }
}

impl IntoError for fmt::Error {
    #[inline]
    fn into_error(self) -> Error {
        Error::from_parts(StatusCode::BAD_REQUEST, None, self)
    }
}

impl IntoError for InvalidStatusCode {
    #[inline]
    fn into_error(self) -> Error {
        Error::from_parts(StatusCode::BAD_REQUEST, None, self)
    }
}

impl Error {
    /// Creates a new [`Error`]
    pub fn new(instance: &str, err: impl Into<BoxError>) -> Self {
        Self::from_parts(
            StatusCode::INTERNAL_SERVER_ERROR,
            Some(instance.into()),
            err,
        )
    }

    /// Creates an internal server error
    #[inline]
    pub fn server_error(err: impl Into<BoxError>) -> Self {
        Self::from_parts(StatusCode::INTERNAL_SERVER_ERROR, None, err)
    }

    /// Creates a client error
    #[inline]
    pub fn client_error(err: impl Into<BoxError>) -> Self {
        Self::from_parts(StatusCode::BAD_REQUEST, None, err)
    }

    /// Creates [`Error`] from status code, instance and underlying error
    #[inline]
    pub fn from_parts(
        status: StatusCode,
        instance: Option<String>,
        err: impl Into<BoxError>,
    ) -> Self {
        Self {
            status,
            inner: err.into(),
            extras: ErrorExtras::of_instance(instance),
        }
    }

    /// Returns HTTP status code of this error
    #[inline]
    pub fn status(&self) -> StatusCode {
        self.status
    }

    /// Returns an instance where this error happened
    #[inline]
    pub fn instance(&self) -> Option<&str> {
        self.extras
            .as_ref()
            .and_then(|extras| extras.instance.as_deref())
    }

    /// Sets the instance where this error happened, unless it has one already
    #[inline]
    pub(crate) fn set_instance_if_none(&mut self, instance: impl FnOnce() -> String) {
        let extras = self.extras.get_or_insert_with(Box::default);

        if extras.instance.is_none() {
            extras.instance = Some(instance());
        }
    }

    /// Unwraps the inner error
    ///
    /// A response attached with [`with_response`](Self::with_response) is dropped.
    pub fn into_inner(self) -> BoxError {
        self.inner
    }

    /// Unwraps the error into a tuple of status code, instance value and underlying error
    ///
    /// A response attached with [`with_response`](Self::with_response) is dropped.
    pub fn into_parts(self) -> (StatusCode, Option<String>, BoxError) {
        let instance = self.extras.and_then(|extras| extras.instance);
        (self.status, instance, self.inner)
    }

    /// Makes the error answer with `response` instead of the response its error handler
    /// would build
    ///
    /// The error is still an error. A handler set with [`map_err`](App::map_err) receives it
    /// and can answer with something else, or return the error to answer with this response.
    /// The default error handler and
    /// [`use_problem_details`](App::use_problem_details) answer with the response unchanged.
    ///
    /// The response takes the error's status, whatever status it was built with, so a body
    /// can be passed alone, such as a [`Json`](crate::Json) value. The status, the instance
    /// and the message stay what they were, for whatever reads the error. If `response`
    /// fails to build, the error is returned without it and answers as it would have.
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, Json, error::{Error, IntoError}, http::StatusCode};
    /// use serde::Serialize;
    ///
    /// #[derive(Serialize)]
    /// struct ErrorBody {
    ///     code: &'static str,
    /// }
    ///
    /// enum ApiError {
    ///     NotFound,
    /// }
    ///
    /// impl IntoError for ApiError {
    ///     fn into_error(self) -> Error {
    ///         match self {
    ///             ApiError::NotFound => Error::from_parts(StatusCode::NOT_FOUND, None, "not found")
    ///                 .with_response(Json(ErrorBody { code: "not_found" })),
    ///         }
    ///     }
    /// }
    ///
    /// # #[tokio::main]
    /// # async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    ///
    /// // 404 with {"code":"not_found"}
    /// app.map_get("/items/{id}", |_id: u64| Err::<Json<u64>, _>(ApiError::NotFound));
    /// # app.run().await
    /// # }
    /// ```
    pub fn with_response(mut self, response: impl IntoResponse) -> Self {
        match response.into_response() {
            Ok(mut response) => {
                *response.status_mut() = self.status;
                self.extras.get_or_insert_with(Box::default).response =
                    Some(AttachedResponse::new(response));
            }
            Err(_err) => {
                #[cfg(feature = "tracing")]
                tracing::warn!("a response attached to an error failed to build: {_err}");
            }
        }
        self
    }

    /// Returns `true` if the error answers with a response of its own; see
    /// [`with_response`](Self::with_response)
    #[inline]
    pub fn has_response(&self) -> bool {
        self.extras
            .as_ref()
            .is_some_and(|extras| extras.response.is_some())
    }

    /// Takes the response the error answers with, leaving the error without one; see
    /// [`with_response`](Self::with_response)
    #[inline]
    pub fn take_response(&mut self) -> Option<HttpResponse> {
        self.extras
            .as_mut()?
            .response
            .take()
            .map(AttachedResponse::into_inner)
    }

    /// Check if the status is within 500-599.
    #[inline]
    pub fn is_server_error(&self) -> bool {
        self.status.is_server_error()
    }

    /// Check if the status is within 400-499.
    #[inline]
    pub fn is_client_error(&self) -> bool {
        self.status.is_client_error()
    }

    #[inline]
    fn from_io_error_fallback(err: IoError) -> Self {
        let status = match err.kind() {
            ErrorKind::NotFound => StatusCode::NOT_FOUND,
            ErrorKind::PermissionDenied => StatusCode::FORBIDDEN,

            ErrorKind::ConnectionRefused
            | ErrorKind::ConnectionReset
            | ErrorKind::ConnectionAborted
            | ErrorKind::NotConnected
            | ErrorKind::AddrInUse
            | ErrorKind::AddrNotAvailable
            | ErrorKind::BrokenPipe => StatusCode::BAD_GATEWAY,

            ErrorKind::AlreadyExists => StatusCode::CONFLICT,
            ErrorKind::InvalidInput | ErrorKind::InvalidData => StatusCode::BAD_REQUEST,
            ErrorKind::TimedOut => StatusCode::REQUEST_TIMEOUT,
            ErrorKind::Unsupported => StatusCode::UNSUPPORTED_MEDIA_TYPE,

            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };

        Self::from_parts(status, None, err)
    }
}

impl App {
    /// Adds a global error handler
    ///
    /// # Example
    /// ```no_run
    ///  use volga::{App, error::Error, status};
    ///
    /// # #[tokio::main]
    /// # async fn main() -> std::io::Result<()> {
    ///  let mut app = App::new();
    ///  
    ///  app.map_err(|error: Error| async move {
    ///     status!(500, { "error_message:": error.to_string() })
    ///  });
    /// # app.run().await
    /// # }
    /// ```
    pub fn map_err<F, R, Args, M>(&mut self, handler: F) -> &mut Self
    where
        F: MapErr<Args, M, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequestParts + Send + 'static,
        M: 'static,
    {
        self.pipeline
            .set_error_handler(ErrorFunc::new(handler).into());
        self
    }

    /// Adds a special fallback handler that handles the unregistered paths
    ///
    /// The fallback sits at the end of the global middleware pipeline, in the
    /// place a matched route's handler would occupy, so the middleware around
    /// it runs as usual and the per-request scope is there to read.
    ///
    /// It answers what nothing else claims: a route group can claim its own
    /// prefix with a fallback of its own -
    /// [`RouteGroup::map_fallback`](crate::routing::RouteGroup::map_fallback) -
    /// and the fallback file of a static file mount answers a `GET` under the
    /// mount's prefix, so neither of them reaches this one.
    ///
    /// It takes the same arguments [`map_err`](Self::map_err) does - anything
    /// implementing [`FromRequestParts`], which covers headers, the URI,
    /// cookies, [`ClientIp`](crate::ClientIp) and [`Dc<T>`](crate::di::Dc).
    /// Not the body: nothing matched, so there is no route to say how a body
    /// should be read. Path parameters are out for the same reason -
    /// [`Path`](crate::Path) and [`NamedPath`](crate::NamedPath) have nothing
    /// to read.
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, error::Error, not_found};
    ///
    /// # #[tokio::main]
    /// # async fn main() -> std::io::Result<()> {
    ///  let mut app = App::new();
    ///  
    ///  app.map_fallback(|| async {
    ///     not_found!()
    ///  });
    /// # app.run().await
    /// # }
    /// ```
    ///
    /// # Example with extractors
    /// ```no_run
    /// use volga::{App, ClientIp, http::Uri, not_found};
    ///
    /// # #[tokio::main]
    /// # async fn main() -> std::io::Result<()> {
    ///  let mut app = App::new();
    ///
    ///  app.map_fallback(|uri: Uri, ip: ClientIp| async move {
    ///     not_found!("no route for {uri} (from {ip})")
    ///  });
    /// # app.run().await
    /// # }
    /// ```
    pub fn map_fallback<F, Args, R, M>(&mut self, handler: F) -> &mut Self
    where
        F: GenericHandler<Args, M, Output = R>,
        Args: FromRequestParts + Send + 'static,
        R: IntoResponse,
        M: 'static,
    {
        self.pipeline
            .set_fallback_handler(FallbackFunc::new(handler).into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{Error, StatusCode};
    use std::io::{Error as IoError, ErrorKind};

    #[test]
    fn it_creates_new_error() {
        let err = Error::new("/api", "some error");

        assert!(err.is_server_error());
        assert_eq!(err.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(err.instance().unwrap(), "/api");
    }

    #[test]
    fn it_converts_from_not_found_io_error() {
        let io_error = IoError::new(ErrorKind::NotFound, "not found");
        let err = Error::from(io_error);

        assert!(err.is_client_error());
        assert_eq!(err.status(), StatusCode::NOT_FOUND);
        assert_eq!(err.instance(), None);
    }

    #[test]
    fn it_converts_from_connection_reset_io_error() {
        let io_error = IoError::new(ErrorKind::ConnectionReset, "reset");
        let err = Error::from(io_error);

        assert!(err.is_server_error());
        assert_eq!(err.status(), StatusCode::BAD_GATEWAY);
        assert_eq!(err.instance(), None);
    }

    #[test]
    fn it_converts_from_connection_aborted_io_error() {
        let io_error = IoError::new(ErrorKind::ConnectionAborted, "aborted");
        let err = Error::from(io_error);

        assert!(err.is_server_error());
        assert_eq!(err.status(), StatusCode::BAD_GATEWAY);
        assert_eq!(err.instance(), None);
    }

    #[test]
    fn it_converts_from_not_connected_io_error() {
        let io_error = IoError::new(ErrorKind::NotConnected, "not connected");
        let err = Error::from(io_error);

        assert!(err.is_server_error());
        assert_eq!(err.status(), StatusCode::BAD_GATEWAY);
        assert_eq!(err.instance(), None);
    }

    #[test]
    fn it_converts_from_add_in_use_io_error() {
        let io_error = IoError::new(ErrorKind::AddrInUse, "addr in use");
        let err = Error::from(io_error);

        assert!(err.is_server_error());
        assert_eq!(err.status(), StatusCode::BAD_GATEWAY);
        assert_eq!(err.instance(), None);
    }

    #[test]
    fn it_converts_from_addr_not_available_io_error() {
        let io_error = IoError::new(ErrorKind::AddrNotAvailable, "addr not available");
        let err = Error::from(io_error);

        assert!(err.is_server_error());
        assert_eq!(err.status(), StatusCode::BAD_GATEWAY);
        assert_eq!(err.instance(), None);
    }

    #[test]
    fn it_converts_from_broken_pipe_io_error() {
        let io_error = IoError::new(ErrorKind::BrokenPipe, "broken pipe");
        let err = Error::from(io_error);

        assert!(err.is_server_error());
        assert_eq!(err.status(), StatusCode::BAD_GATEWAY);
        assert_eq!(err.instance(), None);
    }

    #[test]
    fn it_converts_from_already_exists_io_error() {
        let io_error = IoError::new(ErrorKind::AlreadyExists, "exists");
        let err = Error::from(io_error);

        assert!(err.is_client_error());
        assert_eq!(err.status(), StatusCode::CONFLICT);
        assert_eq!(err.instance(), None);
    }

    #[test]
    fn it_converts_from_invalid_data_io_error() {
        let io_error = IoError::new(ErrorKind::InvalidData, "invalid data");
        let err = Error::from(io_error);

        assert!(err.is_client_error());
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
        assert_eq!(err.instance(), None);
    }

    #[test]
    fn it_converts_from_timed_out_io_error() {
        let io_error = IoError::new(ErrorKind::TimedOut, "timeout");
        let err = Error::from(io_error);

        assert!(err.is_client_error());
        assert_eq!(err.status(), StatusCode::REQUEST_TIMEOUT);
        assert_eq!(err.instance(), None);
    }

    #[test]
    fn it_converts_from_unsupported_io_error() {
        let io_error = IoError::new(ErrorKind::Unsupported, "unsupported");
        let err = Error::from(io_error);

        assert!(err.is_client_error());
        assert_eq!(err.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
        assert_eq!(err.instance(), None);
    }

    #[test]
    fn it_converts_from_permission_denied_io_error() {
        let io_error = IoError::new(ErrorKind::PermissionDenied, "forbidden");
        let err = Error::from(io_error);

        assert!(err.is_client_error());
        assert_eq!(err.status(), StatusCode::FORBIDDEN);
        assert_eq!(err.instance(), None);
    }

    #[test]
    fn it_converts_from_connection_refused_io_error() {
        let io_error = IoError::new(ErrorKind::ConnectionRefused, "connection refused");
        let err = Error::from(io_error);

        assert!(err.is_server_error());
        assert_eq!(err.status(), StatusCode::BAD_GATEWAY);
        assert_eq!(err.instance(), None);
    }

    #[test]
    fn it_converts_from_io_error() {
        let io_error = IoError::other("some error");
        let err = Error::from(io_error);

        assert!(err.is_server_error());
        assert_eq!(err.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(err.instance(), None);
    }

    #[test]
    fn it_converts_error_to_io_error() {
        let error = Error::client_error("some error");
        let io_error = IoError::from(error);

        assert_eq!(io_error.kind(), ErrorKind::Other);
    }

    #[test]
    fn it_splits_into_parts() {
        let error = Error::server_error("some error");

        let (status, instance, inner) = error.into_parts();

        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
        assert!(instance.is_none());
        assert_eq!(format!("{inner}"), "some error");
    }

    #[test]
    fn it_unwraps_into_inner() {
        let error = Error::server_error("some error");

        let inner = error.into_inner();

        assert_eq!(format!("{inner}"), "some error");
    }

    #[test]
    #[allow(clippy::default_constructed_unit_structs)]
    fn it_converts_from_fmt_error() {
        let fmt_error = std::fmt::Error::default();
        let err = Error::from(fmt_error);

        assert!(err.is_client_error());
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
        assert_eq!(err.instance(), None);
    }

    #[test]
    fn it_converts_from_io_error_with_inner_volga_error() {
        let io_error = IoError::other(Error::client_error("some error"));
        let err = Error::from(io_error);

        assert!(err.is_client_error());
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
        assert_eq!(err.instance(), None);
    }

    #[tokio::test]
    async fn it_attaches_a_response_under_its_own_status() {
        use http_body_util::BodyExt;

        let mut err = Error::from_parts(StatusCode::NOT_FOUND, None, "missing")
            .with_response(crate::Json(serde_json::json!({ "code": "missing" })));

        assert!(err.has_response());
        assert_eq!(err.status(), StatusCode::NOT_FOUND);
        assert_eq!(err.to_string(), "missing");

        let mut response = err.take_response().unwrap();

        assert!(!err.has_response());
        assert!(err.take_response().is_none());
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        assert_eq!(response.headers()["content-type"], "application/json");

        let body = response.body_mut().collect().await.unwrap().to_bytes();
        assert_eq!(body, r#"{"code":"missing"}"#);
    }

    #[test]
    fn it_drops_a_response_that_fails_to_build() {
        let err = Error::client_error("bad").with_response(Error::server_error("render"));

        assert!(!err.has_response());
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
        assert_eq!(err.to_string(), "bad");
    }

    #[test]
    fn it_keeps_the_attached_response_through_an_io_error() {
        let err = Error::client_error("bad").with_response("body");
        let err = Error::from(IoError::from(err));

        assert!(err.has_response());
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn it_stays_send_sync_and_small() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<Error>();

        // A status code, the boxed inner error (two words) and the boxed extras (one word):
        // an extractor's `Result<T, Error>` is this large for any `T` no larger than it
        assert!(size_of::<Error>() <= 4 * size_of::<usize>());
        assert_eq!(size_of::<Result<(), Error>>(), size_of::<Error>());
        assert_eq!(size_of::<Result<u32, Error>>(), size_of::<Error>());
        assert_eq!(
            size_of::<crate::HttpResult>(),
            size_of::<crate::HttpResponse>()
        );
    }

    #[test]
    fn it_allocates_no_extras_without_an_instance_or_a_response() {
        let err = Error::server_error("boom");

        assert!(err.extras.is_none());
        assert_eq!(err.instance(), None);
        assert!(!err.has_response());
    }

    #[test]
    fn it_sets_the_instance_only_when_it_has_none() {
        let mut err = Error::server_error("boom");
        err.set_instance_if_none(|| "/first".into());
        err.set_instance_if_none(|| unreachable!("the instance is set already"));

        assert_eq!(err.instance(), Some("/first"));

        let mut err = Error::new("/own", "boom");
        err.set_instance_if_none(|| "/uri".into());

        assert_eq!(err.instance(), Some("/own"));
    }

    #[test]
    fn it_keeps_the_instance_and_the_response_side_by_side() {
        let mut err = Error::client_error("bad").with_response("body");
        err.set_instance_if_none(|| "/uri".into());

        assert!(err.has_response());
        assert_eq!(err.instance(), Some("/uri"));

        assert!(err.take_response().is_some());
        assert_eq!(err.instance(), Some("/uri"));

        let (status, instance, inner) = err.into_parts();

        assert_eq!(status, StatusCode::BAD_REQUEST);
        assert_eq!(instance.as_deref(), Some("/uri"));
        assert_eq!(inner.to_string(), "bad");
    }

    #[test]
    fn it_debugs_as_a_flat_struct() {
        let err = Error::new("/x", "boom");

        assert_eq!(
            format!("{err:?}"),
            r#"Error { status: 500, instance: Some("/x"), inner: "boom", response: None }"#
        );

        let err = Error::client_error("bad").with_response("body");

        assert_eq!(
            format!("{err:?}"),
            r#"Error { status: 400, instance: None, inner: "bad", response: Some(AttachedResponse { .. }) }"#
        );
    }
}
