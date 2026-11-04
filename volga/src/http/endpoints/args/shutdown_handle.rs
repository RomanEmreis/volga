//! Extractors for [`ShutdownHandle`]

use hyper::http::Extensions;

use crate::http::{FromRequestHead, RequestHead, endpoints::args::sealed::PathAccess};
use crate::{ShutdownHandle, error::Error, http::request_scope::HttpRequestScope};

impl TryFrom<&Extensions> for ShutdownHandle {
    type Error = Error;

    #[inline]
    fn try_from(extensions: &Extensions) -> Result<Self, Self::Error> {
        // A handle that never fires would leave a stream waiting on it running to the end of
        // the shutdown, so a request that did not come through a running server is an error.
        // The server's token is cloned here, for the handler that asked for it, rather than
        // for every request - see `HttpRequestScope::shutdown`
        extensions
            .get::<HttpRequestScope>()
            .map(|scope| ShutdownHandle::clone(&scope.shutdown))
            .ok_or_else(|| Error::server_error("Server Error: shutdown handle is not available"))
    }
}

/// Extracts [`ShutdownHandle`] from the head of a request
impl FromRequestHead for ShutdownHandle {
    #[inline]
    fn from_head(head: &RequestHead<'_>) -> Result<Self, Error> {
        Self::try_from(head.extensions())
    }

    // Reads no path parameter, so a positional extractor next to it takes its own
    const PATH_ACCESS: PathAccess = PathAccess::Ignored;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::HttpBody;
    use crate::HttpRequest;
    use crate::http::FromRequestRef;
    use crate::http::endpoints::args::FromPayload;
    use crate::http::endpoints::args::Payload;
    use hyper::Request;

    fn scope_with(shutdown: ShutdownHandle) -> HttpRequestScope {
        HttpRequestScope {
            shutdown: std::sync::Arc::new(shutdown),
            ..HttpRequestScope::default()
        }
    }

    #[tokio::test]
    async fn it_reads_from_payload() {
        let handle = ShutdownHandle::new();
        let (parts, _) = Request::get("/")
            .extension(scope_with(handle.clone()))
            .body(())
            .unwrap()
            .into_parts();

        let extracted = ShutdownHandle::from_payload(Payload::Parts(&parts))
            .await
            .unwrap();
        handle.shutdown();

        assert!(extracted.is_shutdown_requested());
    }

    #[test]
    fn it_gets_from_request_ref() {
        let handle = ShutdownHandle::new();
        handle.shutdown();

        let (parts, body) = Request::get("/")
            .extension(scope_with(handle))
            .body(HttpBody::empty())
            .unwrap()
            .into_parts();
        let req = HttpRequest::from_parts(parts, body);

        assert!(
            ShutdownHandle::from_request(&RequestHead::from_request(&req))
                .unwrap()
                .is_shutdown_requested()
        );
    }

    #[test]
    fn it_fails_without_request_scope() {
        let (parts, _) = Request::get("/").body(()).unwrap().into_parts();

        let err = ShutdownHandle::from_request(&RequestHead::from_parts(&parts)).unwrap_err();

        assert_eq!(
            err.to_string(),
            "Server Error: shutdown handle is not available"
        );
    }
}
