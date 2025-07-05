//! Tools and utilities for filter and validation results.

use std::ops::{Deref, DerefMut};
use crate::error::{BoxError, Error};
use crate::http::IntoResponse;
use crate::HttpResult;

/// Result of filter or validation middleware.
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
        if value { 
            Self::ok()
        } else { 
            Self::err()
        }
    }
}

impl FilterResult {
    /// Creates a new, valid [`FilterResult`].
    #[inline]
    pub fn ok() -> Self {
        Self(Ok(()))
    }

    /// Creates a new, invalid [`FilterResult`].
    #[inline]
    pub fn err() -> Self {
        Self(Err(Error::client_error("Validation: One or more request parameters are incorrect")))
    }
    
    /// Unwraps the inner result.
    #[inline]
    pub fn into_inner(self) -> Result<(), Error> {
        self.0
    }
    
    /// Updates the result with the given error.
    #[inline]
    pub fn with_error(mut self, error: impl Into<BoxError>) -> Self {
        self.0 = Err(Error::client_error(error));
        self
    }
}
