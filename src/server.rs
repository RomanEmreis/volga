//! HTTP Server tools

use hyper::rt::{Read, Write};

#[cfg(all(feature = "http1", not(feature = "http2")))]
pub(super) mod http1;
#[cfg(any(
    all(feature = "http1", feature = "http2"),
    all(feature = "http2", not(feature = "http1"))
))]
pub(super) mod http2;

pub(super) struct Server<I: Read + Write + Unpin> {
    io: I
}

