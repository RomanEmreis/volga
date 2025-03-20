//! Base HTTP tools

// Re-exporting HTTP status codes, Response and some headers from hyper/http
pub use hyper::{
    http::{Uri, Method, Extensions},
    Response, 
    StatusCode,
};

pub use body::{BoxBody, UnsyncBoxBody, HttpBody};
pub use endpoints::{
    args::{FromRawRequest, FromRequestRef, FromRequest, FromRequestParts},
    handlers::GenericHandler
};
pub use request::HttpRequest;
pub use response::{
    into_response::IntoResponse,
    HttpHeaders,
    HttpResponse,
    HttpResult,
    ResponseContext,
    Results
};

#[cfg(feature = "middleware")]
pub use cors::CorsConfig;

pub mod body;
pub mod request;
pub mod response;
pub mod endpoints;
#[cfg(feature = "middleware")]
pub mod cors;