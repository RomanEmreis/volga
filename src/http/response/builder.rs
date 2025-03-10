﻿pub const SERVER_NAME: &str = "Volga";
pub const RESPONSE_ERROR: &str = "HTTP Response: Unable to create a response";

/// Creates a default HTTP response builder
#[macro_export]
macro_rules! builder {
    () => {
        $crate::http::Response::builder()
            .header($crate::headers::SERVER, $crate::SERVER_NAME)
    };
    ($status:expr) => {
        $crate::builder!()
            .status($status)
    };
}

/// Creates an HTTP response with `status`, `body` and `headers`
#[macro_export]
macro_rules! response {
    ($status:expr, $body:expr) => {
        $crate::response!($status, $body, [])
    };
    ($status:expr, $body:expr, [ $( ($key:expr, $value:expr) ),* $(,)? ]) => {
        $crate::builder!($status)
        $(
            .header($key, $value)
        )*
            .body($body)
            .map_err(|_| $crate::error::Error::server_error($crate::RESPONSE_ERROR))
    };
}
