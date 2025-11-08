//! Tools and utilities for filter and validation results.

use std::ops::{Deref, DerefMut};
use crate::error::{BoxError, Error};
use crate::http::IntoResponse;
use crate::HttpResult;

/// Result of filter or validation middleware.
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

#[cfg(test)]
mod tests {
    use super::*;

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