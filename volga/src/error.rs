//! Error Handling tools

use std::{
    convert::Infallible,
    fmt,
    io::{ErrorKind, Error as IoError},
    error::Error as StdError
};

use super::{
    App,
    http::{
        GenericHandler,
        MapErrHandler,
        FromRequestParts,
        FromRawRequest,
        StatusCode,
        IntoResponse
    }
};

pub use self::{
    handler::{ErrorHandler, ErrorFunc},
    fallback::{FallbackHandler, FallbackFunc}
};

#[cfg(feature = "problem-details")]
pub use self::problem::{Problem, ProblemDetails};

pub mod handler;
pub mod fallback;
#[cfg(feature = "problem-details")]
pub mod problem;

pub(crate) type BoxError = Box<
    dyn StdError 
    + Send 
    + Sync
>;

/// Generic error
#[derive(Debug)]
pub struct Error {
    /// HTTP status code
    pub status: StatusCode,

    /// An instance where this error happened
    pub instance: Option<String>,

    /// Inner error object
    pub(crate) inner: BoxError,
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

impl From<Infallible> for Error {
    fn from(infallible: Infallible) -> Error {
        match infallible {}
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Error {
        Self {
            status: StatusCode::BAD_REQUEST,
            inner: err.into(),
            instance: None,
        }
    }
}

impl From<IoError> for Error {
    #[inline]
    fn from(err: IoError) -> Self {
        let status = match err.kind() { 
            ErrorKind::NotFound => StatusCode::NOT_FOUND,
            ErrorKind::PermissionDenied => StatusCode::FORBIDDEN,
            ErrorKind::ConnectionRefused => StatusCode::BAD_GATEWAY,
            ErrorKind::ConnectionReset => StatusCode::BAD_GATEWAY,
            ErrorKind::ConnectionAborted => StatusCode::BAD_GATEWAY,
            ErrorKind::NotConnected => StatusCode::BAD_GATEWAY,
            ErrorKind::AddrInUse => StatusCode::BAD_GATEWAY,
            ErrorKind::AddrNotAvailable => StatusCode::BAD_GATEWAY,
            ErrorKind::BrokenPipe => StatusCode::BAD_GATEWAY,
            ErrorKind::AlreadyExists => StatusCode::CONFLICT,
            ErrorKind::InvalidInput => StatusCode::BAD_REQUEST,
            ErrorKind::InvalidData => StatusCode::BAD_REQUEST,
            ErrorKind::TimedOut => StatusCode::REQUEST_TIMEOUT,
            ErrorKind::Unsupported => StatusCode::UNSUPPORTED_MEDIA_TYPE,
            _ => StatusCode::INTERNAL_SERVER_ERROR
        };
        
        Self { 
            instance: None,
            inner: err.into(),
            status
        }
    }
}

impl From<hyper::http::Error> for Error {
    #[inline]
    fn from(err: hyper::http::Error) -> Self {
        Self {
            instance: None,
            inner: err.into(),
            status: StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

impl From<Error> for IoError {
    #[inline]
    fn from(err: Error) -> Self {
        Self::other(err)
    }
}

impl From<fmt::Error> for Error {
    #[inline]
    fn from(err: fmt::Error) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            inner: err.into(),
            instance: None,
        }
    }
}

impl Error {
    /// Creates a new [`Error`]
    pub fn new(instance: &str, err: impl Into<BoxError>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            inner: err.into(),
            instance: Some(instance.into())
        }
    }
    
    /// Creates an internal server error
    #[inline]
    pub fn server_error(err: impl Into<BoxError>) -> Self {
        Self {
            status: StatusCode::INTERNAL_SERVER_ERROR,
            inner: err.into(),
            instance: None,
        }
    }

    /// Creates a client error
    #[inline]
    pub fn client_error(err: impl Into<BoxError>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            inner: err.into(),
            instance: None,
        }
    }
    
    /// Creates [`Error`] from status code, instance and underlying error
    #[inline]
    pub fn from_parts(status: StatusCode, instance: Option<String>, err: impl Into<BoxError>) -> Self {
        Self { status, instance, inner: err.into() }
    }
    
    /// Unwraps the inner error
    pub fn into_inner(self) -> BoxError {
        self.inner
    }
    
    /// Unwraps the error into a tuple of status code, instance value and underlying error
    pub fn into_parts(self) -> (StatusCode, Option<String>, BoxError) {
        (self.status, self.instance, self.inner)
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
}

impl App {
    /// Adds a global error handler
    /// 
    /// # Example
    /// ```no_run
    /// use volga::{App, error::Error, status};
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
    pub fn map_err<F, R, Args>(&mut self, handler: F) -> &mut Self
    where
        F: MapErrHandler<Args, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequestParts + Send + Sync + 'static,
    {
        self.pipeline
            .set_error_handler(ErrorFunc::new(handler).into());
        self
    }

    /// Adds a special fallback handler that handles the unregistered paths
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
    pub fn map_fallback<F, Args, R>(&mut self, handler: F) -> &mut Self
    where
        F: GenericHandler<Args, Output = R>,
        Args: FromRawRequest + Send + Sync + 'static,
        R: IntoResponse
    {
        self.pipeline
            .set_fallback_handler(FallbackFunc::new(handler).into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::{Error, StatusCode};
    use std::io::{ErrorKind, Error as IoError};

    #[test]
    fn it_creates_new_error() {
        let err = Error::new("/api", "some error");

        assert!(err.is_server_error());
        assert_eq!(err.status, StatusCode::INTERNAL_SERVER_ERROR);
    }
    
    #[test]
    fn it_converts_from_not_found_io_error() {
        let io_error = IoError::new(ErrorKind::NotFound, "not found");
        let err = Error::from(io_error);
        
        assert!(err.is_client_error());
        assert_eq!(err.status, StatusCode::NOT_FOUND);
    }

    #[test]
    fn it_converts_from_connection_reset_io_error() {
        let io_error = IoError::new(ErrorKind::ConnectionReset, "reset");
        let err = Error::from(io_error);

        assert!(err.is_server_error());
        assert_eq!(err.status, StatusCode::BAD_GATEWAY);
    }

    #[test]
    fn it_converts_from_connection_aborted_io_error() {
        let io_error = IoError::new(ErrorKind::ConnectionAborted, "aborted");
        let err = Error::from(io_error);

        assert!(err.is_server_error());
        assert_eq!(err.status, StatusCode::BAD_GATEWAY);
    }

    #[test]
    fn it_converts_from_not_connected_io_error() {
        let io_error = IoError::new(ErrorKind::NotConnected, "not connected");
        let err = Error::from(io_error);

        assert!(err.is_server_error());
        assert_eq!(err.status, StatusCode::BAD_GATEWAY);
    }

    #[test]
    fn it_converts_from_add_in_use_io_error() {
        let io_error = IoError::new(ErrorKind::AddrInUse, "addr in use");
        let err = Error::from(io_error);

        assert!(err.is_server_error());
        assert_eq!(err.status, StatusCode::BAD_GATEWAY);
    }

    #[test]
    fn it_converts_from_addr_not_available_io_error() {
        let io_error = IoError::new(ErrorKind::AddrNotAvailable, "addr not available");
        let err = Error::from(io_error);

        assert!(err.is_server_error());
        assert_eq!(err.status, StatusCode::BAD_GATEWAY);
    }

    #[test]
    fn it_converts_from_broken_pipe_io_error() {
        let io_error = IoError::new(ErrorKind::BrokenPipe, "broken pipe");
        let err = Error::from(io_error);

        assert!(err.is_server_error());
        assert_eq!(err.status, StatusCode::BAD_GATEWAY);
    }

    #[test]
    fn it_converts_from_already_exists_io_error() {
        let io_error = IoError::new(ErrorKind::AlreadyExists, "exists");
        let err = Error::from(io_error);

        assert!(err.is_client_error());
        assert_eq!(err.status, StatusCode::CONFLICT);
    }

    #[test]
    fn it_converts_from_invalid_data_io_error() {
        let io_error = IoError::new(ErrorKind::InvalidData, "invalid data");
        let err = Error::from(io_error);

        assert!(err.is_client_error());
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn it_converts_from_timed_out_io_error() {
        let io_error = IoError::new(ErrorKind::TimedOut, "timeout");
        let err = Error::from(io_error);

        assert!(err.is_client_error());
        assert_eq!(err.status, StatusCode::REQUEST_TIMEOUT);
    }

    #[test]
    fn it_converts_from_unsupported_io_error() {
        let io_error = IoError::new(ErrorKind::Unsupported, "unsupported");
        let err = Error::from(io_error);

        assert!(err.is_client_error());
        assert_eq!(err.status, StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    #[test]
    fn it_converts_from_permission_denied_io_error() {
        let io_error = IoError::new(ErrorKind::PermissionDenied, "forbidden");
        let err = Error::from(io_error);

        assert!(err.is_client_error());
        assert_eq!(err.status, StatusCode::FORBIDDEN);
    }

    #[test]
    fn it_converts_from_connection_refused_io_error() {
        let io_error = IoError::new(ErrorKind::ConnectionRefused, "connection refused");
        let err = Error::from(io_error);

        assert!(err.is_server_error());
        assert_eq!(err.status, StatusCode::BAD_GATEWAY);
    }

    #[test]
    fn it_converts_from_io_error() {
        let io_error = IoError::other("some error");
        let err = Error::from(io_error);

        assert!(err.is_server_error());
        assert_eq!(err.status, StatusCode::INTERNAL_SERVER_ERROR);
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
        assert_eq!(err.status, StatusCode::BAD_REQUEST);
    }
}
