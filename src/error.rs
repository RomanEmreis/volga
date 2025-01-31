//! Error Handling tools

use super::{App, http::{StatusCode, IntoResponse}};

use std::{
    convert::Infallible,
    fmt,
    future::Future, 
    io::Error as IoError,
    error::Error as StdError
};
use std::io::ErrorKind;
pub use self::handler::{ErrorHandler, ErrorFunc};

#[cfg(feature = "problem-details")]
pub use self::problem::Problem;

pub mod handler;
#[cfg(feature = "problem-details")]
pub mod problem;

type BoxError = Box<
    dyn StdError 
    + Send 
    + Sync
>;

/// Generic error
#[derive(Debug)]
pub struct Error {
    pub status: StatusCode,
    pub instance: Option<String>,
    pub(crate) inner: BoxError
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
            instance: None
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

impl From<Error> for IoError {
    #[inline]
    fn from(err: Error) -> Self {
        Self::other(err)
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
            instance: None
        }
    }

    /// Creates a client error
    #[inline]
    pub fn client_error(err: impl Into<BoxError>) -> Self {
        Self {
            status: StatusCode::BAD_REQUEST,
            inner: err.into(),
            instance: None
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
    
    /// Check if status is within 500-599.
    #[inline]
    pub fn is_server_error(&self) -> bool {
        self.status.is_server_error()
    }

    /// Check if status is within 400-499.
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
    pub fn map_err<F, R, Fut>(&mut self, handler: F) -> &mut Self
    where
        F: Fn(Error) -> Fut + Send + Sync + 'static,
        R: IntoResponse,
        Fut: Future<Output = R> + Send
    {
        self.pipeline
            .set_error_handler(ErrorFunc(handler).into());
        self
    }
}