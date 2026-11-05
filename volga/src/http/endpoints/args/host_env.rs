//! Extractors for the whole Hosting Environment request

use crate::http::{FromRequestHead, RequestHead, endpoints::args::sealed::PathAccess};
use crate::{app::HostEnv, error::Error};
use hyper::http::Extensions;

impl TryFrom<&Extensions> for HostEnv {
    type Error = Error;

    #[inline]
    fn try_from(extensions: &Extensions) -> Result<Self, Self::Error> {
        match extensions.get::<HostEnv>() {
            Some(env) => Ok(env.clone()),
            None => Err(Error::server_error(
                "Server Error: hosting environment is not specified",
            )),
        }
    }
}

impl FromRequestHead for HostEnv {
    #[inline]
    fn from_head(head: &RequestHead<'_>) -> Result<Self, Error> {
        head.extensions().try_into()
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
    use crate::http::endpoints::args::Source;
    use hyper::Request;

    #[test]
    fn it_returns_host_env_when_present_in_extensions() {
        let mut ext = Extensions::new();
        ext.insert(HostEnv::new("root"));

        let result = HostEnv::try_from(&ext).unwrap();
        assert_eq!(result, HostEnv::new("root"));
    }

    #[test]
    fn it_returns_error_when_hostenv_missing_in_extensions() {
        let ext = Extensions::new();
        let err = HostEnv::try_from(&ext).unwrap_err();
        assert_eq!(
            err.to_string(),
            "Server Error: hosting environment is not specified"
        );
    }

    #[test]
    fn it_from_request_ref_extracts_hostenv() {
        let (parts, body) = Request::get("/")
            .extension(HostEnv::new("root"))
            .body(HttpBody::empty())
            .unwrap()
            .into_parts();

        let req = HttpRequest::from_parts(parts, body);

        let result = HostEnv::from_request(&RequestHead::from_request(&req)).unwrap();
        assert_eq!(result, HostEnv::new("root"));
    }

    #[test]
    fn it_from_request_ref_returns_error_when_missing() {
        let (parts, body) = Request::get("/")
            .body(HttpBody::empty())
            .unwrap()
            .into_parts();

        let req = HttpRequest::from_parts(parts, body);

        let err = HostEnv::from_request(&RequestHead::from_request(&req)).unwrap_err();

        assert_eq!(
            err.to_string(),
            "Server Error: hosting environment is not specified"
        );
    }

    #[test]
    fn it_from_parts_extracts_hostenv() {
        let (parts, _) = Request::get("/")
            .extension(HostEnv::new("root"))
            .body(HttpBody::empty())
            .unwrap()
            .into_parts();

        let result = HostEnv::from_request(&RequestHead::from_parts(&parts)).unwrap();
        assert_eq!(result, HostEnv::new("root"));
    }

    #[test]
    fn it_from_parts_returns_error_when_missing() {
        let (parts, _) = Request::get("/")
            .body(HttpBody::empty())
            .unwrap()
            .into_parts();

        let err = HostEnv::from_request(&RequestHead::from_parts(&parts)).unwrap_err();
        assert_eq!(
            err.to_string(),
            "Server Error: hosting environment is not specified"
        );
    }

    #[tokio::test]
    async fn it_from_payload_resolves_correctly() {
        let (parts, _) = Request::get("/")
            .extension(HostEnv::new("root"))
            .body(HttpBody::empty())
            .unwrap()
            .into_parts();

        let result = HostEnv::from_payload(Payload::Parts(RequestHead::from_parts(&parts)))
            .await
            .unwrap();
        assert_eq!(result, HostEnv::new("root"));
    }

    #[test]
    fn it_source_returns_parts_variant() {
        assert!(matches!(HostEnv::SOURCE, Source::Parts));
    }
}
