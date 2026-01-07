//! Tools for HTTP headers

use crate::error::Error;

// Re-exporting HeaderMap, HeaderValue and some headers from hyper
pub use hyper::{
    header::{
        InvalidHeaderName,
        InvalidHeaderValue,
        MaxSizeReached,
        ToStrError,
        ACCEPT_ENCODING, ACCEPT_RANGES,
        ACCESS_CONTROL_ALLOW_CREDENTIALS, ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS,
        ACCESS_CONTROL_ALLOW_ORIGIN, ACCESS_CONTROL_EXPOSE_HEADERS, ACCESS_CONTROL_MAX_AGE,
        ACCESS_CONTROL_REQUEST_HEADERS, ACCESS_CONTROL_REQUEST_METHOD,
        AUTHORIZATION,
        CACHE_CONTROL,
        CONTENT_DISPOSITION, CONTENT_ENCODING, CONTENT_LENGTH, CONTENT_RANGE, CONTENT_TYPE,
        ETAG, FORWARDED,
        IF_NONE_MATCH, IF_MODIFIED_SINCE,
        LAST_MODIFIED,
        LOCATION,
        ORIGIN,
        SEC_WEBSOCKET_KEY, SEC_WEBSOCKET_ACCEPT, SEC_WEBSOCKET_PROTOCOL, SEC_WEBSOCKET_VERSION,
        SERVER,
        STRICT_TRANSPORT_SECURITY,
        TRANSFER_ENCODING,
        VARY,
        UPGRADE,
        CONNECTION,
        COOKIE, SET_COOKIE,
        WWW_AUTHENTICATE
    },
    http::{HeaderName, HeaderValue},
    HeaderMap
};

pub use self::{
    super::http::StatusCode,
    etag::ETag,
    cache_control::{CacheControl, ResponseCaching},
    encoding::Encoding,
    extract::*,
    header::{Header, HttpHeaders},
    quality::Quality,
    macros::custom_headers
};

#[cfg(feature = "macros")]
pub use volga_macros::http_header;

pub(crate) mod helpers;
pub mod extract;
pub mod encoding;
pub mod header;
pub mod macros;
pub mod quality;
pub mod etag;
pub mod cache_control;

/// Describes a way to extract a specific HTTP header
pub trait FromHeaders: Clone {
    /// Returns current [`HeaderName`]
    const NAME: HeaderName;
    
    /// Reads a [`HeaderValue`] from [`HeaderMap`]
    fn from_headers(headers: &HeaderMap) -> Option<&HeaderValue>;
}

pub(crate) struct HeaderError;
impl HeaderError {
    #[inline]
    pub(crate) fn header_missing<T: FromHeaders>() -> Error {
        Self::header_missing_impl(T::NAME.as_str())
    }

    #[inline]
    pub(crate) fn header_missing_impl(header: &str) -> Error {
        Error::from_parts(
            StatusCode::NOT_FOUND,
            None,
            format!("Header: `{header}` not found")
        )
    }

    #[inline]
    pub(crate) fn from_invalid_header_value(error: InvalidHeaderValue) -> Error {
        Error::client_error(format!("Header: {error}"))
    }

    #[inline]
    pub(crate) fn from_invalid_header_name(error: InvalidHeaderName) -> Error {
        Error::client_error(format!("Header: {error}"))
    }

    #[inline]
    pub(crate) fn from_to_str_error(error: ToStrError) -> Error {
        Error::client_error(format!("Header: {error}"))
    }

    #[inline]
    pub(crate) fn from_max_size_reached(error: MaxSizeReached) -> Error {
        Error::client_error(format!("Header: {error}"))
    }
}

impl core::convert::From<InvalidHeaderValue> for Error {
    #[inline]
    fn from(error: InvalidHeaderValue) -> Self {
        HeaderError::from_invalid_header_value(error)
    }
}

impl core::convert::From<ToStrError> for Error {
    #[inline]
    fn from(error: ToStrError) -> Self {
        HeaderError::from_to_str_error(error)
    }
}
