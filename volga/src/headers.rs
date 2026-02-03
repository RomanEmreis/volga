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
        ETAG, FORWARDED, HOST,
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
    header::{Header, HttpHeaders, TryIntoHeaderPair},
    quality::Quality,
    macros::headers
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

/// Identifying the originating IP address of a client connecting to a web server through a proxy server.
pub const X_FORWARDED_FOR: HeaderName = HeaderName::from_static("x-forwarded-for");

/// Controls proxy response buffering (required for SSE).
pub const X_ACCEL_BUFFERING: HeaderName = HeaderName::from_static("x-accel-buffering");

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
    fn from_invalid_header_value(error: InvalidHeaderValue) -> Error {
        Error::client_error(format!("Header: {error}"))
    }

    #[inline]
    fn from_invalid_header_name(error: InvalidHeaderName) -> Error {
        Error::client_error(format!("Header: {error}"))
    }

    #[inline]
    fn from_to_str_error(error: ToStrError) -> Error {
        Error::client_error(format!("Header: {error}"))
    }

    #[inline]
    fn from_max_size_reached(error: MaxSizeReached) -> Error {
        Error {
            status: StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE,
            inner: format!("Header: {error}").into(),
            instance: None,
        }
    }
}

impl core::convert::From<InvalidHeaderValue> for Error {
    #[inline]
    fn from(error: InvalidHeaderValue) -> Self {
        HeaderError::from_invalid_header_value(error)
    }
}

impl core::convert::From<InvalidHeaderName> for Error {
    #[inline]
    fn from(error: InvalidHeaderName) -> Self {
        HeaderError::from_invalid_header_name(error)
    }
}

impl core::convert::From<MaxSizeReached> for Error {
    #[inline]
    fn from(error: MaxSizeReached) -> Self {
        HeaderError::from_max_size_reached(error)
    }
}

impl core::convert::From<ToStrError> for Error {
    #[inline]
    fn from(error: ToStrError) -> Self {
        HeaderError::from_to_str_error(error)
    }
}

#[cfg(test)]
#[allow(unreachable_pub)]
mod tests {
    use super::*;
    use crate::http::StatusCode;

    headers! {
        (XTest, "x-test")
    }

    #[test]
    fn header_missing_impl_builds_not_found_error() {
        let err = HeaderError::header_missing_impl("x-test");

        assert_eq!(err.status, StatusCode::NOT_FOUND);

        let msg = err.to_string();
        assert!(msg.contains("Header: `x-test` not found"));
    }

    #[test]
    fn header_missing_uses_from_headers_name() {
        let err = HeaderError::header_missing::<XTest>();

        assert_eq!(err.status, StatusCode::NOT_FOUND);
        let msg = err.to_string();
        assert!(msg.contains("Header: `x-test` not found"));
    }

    #[test]
    fn invalid_header_value_maps_to_client_error() {
        use crate::headers::HeaderValue;

        let invalid = HeaderValue::from_bytes(&[0]).unwrap_err();
        let err: Error = invalid.into();

        assert_eq!(err.status, StatusCode::BAD_REQUEST);

        let msg = err.to_string();
        assert!(msg.contains("Header:"));
    }

    #[test]
    fn invalid_header_name_maps_to_client_error() {
        let invalid = HeaderName::from_bytes(b"Bad Header").unwrap_err();
        let err: Error = invalid.into();

        assert_eq!(err.status, StatusCode::BAD_REQUEST);

        let msg = err.to_string();
        assert!(msg.contains("Header:"));
    }

    #[test]
    fn to_str_error_maps_to_client_error() {
        use crate::headers::HeaderValue;

        let hv = HeaderValue::from_bytes(&[0xFF]).unwrap();
        let to_str_err = hv.to_str().unwrap_err();

        let err: Error = to_str_err.into();

        assert_eq!(err.status, StatusCode::BAD_REQUEST);
        let msg = err.to_string();
        assert!(msg.contains("Header:"));
    }

}
