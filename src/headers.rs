//! Tools for HTTP headers

// Re-exporting HeaderMap, HeaderValue and some headers from hyper
pub use hyper::{
    header::{
        InvalidHeaderValue,
        ToStrError,
        ACCEPT_ENCODING,
        ACCEPT_RANGES,
        CACHE_CONTROL,
        CONTENT_DISPOSITION,
        CONTENT_ENCODING,
        CONTENT_LENGTH,
        CONTENT_RANGE,
        CONTENT_TYPE,
        ETAG,
        IF_NONE_MATCH,
        IF_MODIFIED_SINCE,
        LAST_MODIFIED,
        LOCATION,
        SEC_WEBSOCKET_KEY,
        SEC_WEBSOCKET_ACCEPT,
        SEC_WEBSOCKET_PROTOCOL,
        SEC_WEBSOCKET_VERSION,
        SERVER,
        STRICT_TRANSPORT_SECURITY,
        TRANSFER_ENCODING,
        VARY,
        UPGRADE,
        CONNECTION
    },
    http::HeaderValue,
    HeaderMap
};

pub use self::{
    super::{error::Error, http::StatusCode},
    etag::ETag,
    cache_control::{CacheControl, ResponseCaching},
    encoding::Encoding,
    extract::*,
    header::{Header, Headers},
    quality::Quality,
    macros::custom_headers
};

pub(crate) mod helpers;
pub mod extract;
pub mod encoding;
pub mod header;
pub mod macros;
pub mod quality;
pub mod etag;
pub mod cache_control;

/// Describes a way to extract a specific HTTP header
pub trait FromHeaders {
    /// Reads a [`HeaderValue`] from [`HeaderMap`]
    fn from_headers(headers: &HeaderMap) -> Option<&HeaderValue>;

    /// Returns a header type as `&str`
    fn header_type() -> &'static str;
}

struct HeaderError;
impl HeaderError {
    #[inline]
    fn header_missing<T: FromHeaders>() -> Error {
        Error::from_parts(
            StatusCode::NOT_FOUND, 
            None, 
            format!("Header: `{}` not found", T::header_type())
        )
    }

    #[inline]
    fn from_invalid_header_value(error: InvalidHeaderValue) -> Error {
        Error::client_error(format!("Header: {}", error))
    }

    #[inline]
    fn from_to_str_error(error: ToStrError) -> Error {
        Error::client_error(format!("Header: {}", error))
    }
}
