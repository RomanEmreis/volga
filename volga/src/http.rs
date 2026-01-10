//! Base HTTP tools

// Re-exporting HTTP status codes, headers, method and etc. from hyper/http
pub use hyper::{
    http::{Method, Uri, Version},
    StatusCode,
};

pub(crate) use hyper::{
    http::{request::Parts, Extensions},
    Request, Response
};

pub use body::{BoxBody, HttpBody, UnsyncBoxBody};
pub use endpoints::{
    args::{FromRawRequest, FromRequest, FromRequestParts, FromRequestRef, sse},
    handlers::{GenericHandler, MapErrHandler}
};
pub use request::HttpRequest;
#[cfg(feature = "middleware")]
pub use request::HttpRequestMut;

pub use response::{
    into_response::IntoResponse,
    HttpResponse,
    HttpResult,
};

#[cfg(feature = "middleware")]
pub use response::filter_result::FilterResult;

#[cfg(feature = "middleware")]
pub use cors::CorsConfig;
#[cfg(feature = "cookie")]
pub use cookie::Cookies;
#[cfg(feature = "signed-cookie")]
pub use cookie::signed::{SignedKey, SignedCookies};
#[cfg(feature = "private-cookie")]
pub use cookie::private::{PrivateKey, PrivateCookies};

pub mod body;
pub mod request;
pub mod response;
pub mod endpoints;
#[cfg(feature = "cookie")]
pub mod cookie;
#[cfg(feature = "middleware")]
pub mod cors;
