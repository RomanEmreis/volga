﻿//! Base HTTP tools

// Re-exporting HTTP status codes, Response and some headers from hyper/http
pub use hyper::{
    http::{Extensions, Method, Uri},
    Response,
    StatusCode,
};

pub use body::{BoxBody, HttpBody, UnsyncBoxBody};
pub use endpoints::{
    args::{FromRawRequest, FromRequest, FromRequestParts, FromRequestRef, sse},
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
