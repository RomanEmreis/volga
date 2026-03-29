//! Base HTTP tools

// Re-exporting HTTP status codes, headers, method and etc. from hyper/http
pub use hyper::{
    StatusCode,
    http::{Method, Uri, Version},
};

pub(crate) use hyper::{
    Request, Response,
    http::{Extensions, request::Parts},
};

pub use body::{BoxBody, HttpBody, HttpBodyStream, UnsyncBoxBody};
pub use endpoints::{
    args::{
        FromRawRequest, FromRequest, FromRequestParts, FromRequestRef, byte_stream::IntoByteResult,
        sse,
    },
    handlers::{GenericHandler, MapErrHandler},
};
pub use request::HttpRequest;

#[cfg(feature = "middleware")]
pub use request::HttpRequestMut;

pub use response::{HttpResponse, HttpResult, into_response::IntoResponse};

#[cfg(feature = "middleware")]
pub use response::filter_result::FilterResult;

#[cfg(feature = "cookie")]
pub use cookie::Cookies;
#[cfg(feature = "private-cookie")]
pub use cookie::private::{PrivateCookies, PrivateKey};
#[cfg(feature = "signed-cookie")]
pub use cookie::signed::{SignedCookies, SignedKey};
#[cfg(feature = "middleware")]
pub use cors::CorsConfig;

pub mod body;
#[cfg(feature = "cookie")]
pub mod cookie;
#[cfg(feature = "middleware")]
pub mod cors;
pub mod endpoints;
pub mod request;
pub(crate) mod request_scope;
pub mod response;
