//! Extractors for the whole Hosting Environment request

use crate::{app::HostEnv, error::Error, HttpRequest};
use futures_util::future::{ready, Ready};
use hyper::http::{request::Parts, Extensions};
use crate::http::endpoints::args::{
    FromRequestParts,
    FromRequestRef,
    FromPayload,
    Payload,
    Source
};

impl TryFrom<&Extensions> for HostEnv {
    type Error = Error;
    
    #[inline]
    fn try_from(extensions: &Extensions) -> Result<Self, Self::Error> {
        match extensions.get::<HostEnv>() {
            Some(env) => Ok(env.clone()),
            None => Err(Error::server_error("Server Error: hosting environment is not specified"))
        }
    }
}

impl FromRequestRef for HostEnv {
    #[inline]
    fn from_request(req: &HttpRequest) -> Result<Self, Error> {
        req.extensions().try_into()
    }
}

impl FromRequestParts for HostEnv {
    #[inline]
    fn from_parts(parts: &Parts) -> Result<Self, Error> {
        HostEnv::try_from(&parts.extensions)
    }
}

impl FromPayload for HostEnv {
    type Future = Ready<Result<Self, Error>>;

    const SOURCE: Source = Source::Parts;
    
    #[inline]
    fn from_payload(payload: Payload<'_>) -> Self::Future {
        let Payload::Parts(parts) = payload else { unreachable!() };
        ready(HostEnv::from_parts(parts))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use hyper::Request;
    use crate::HttpBody;

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

        let result = HostEnv::from_request(&req).unwrap();
        assert_eq!(result, HostEnv::new("root"));
    }

    #[test]
    fn it_from_request_ref_returns_error_when_missing() {
        let (parts, body) = Request::get("/")
            .body(HttpBody::empty())
            .unwrap()
            .into_parts();

        let req = HttpRequest::from_parts(parts, body);

        let err = HostEnv::from_request(&req).unwrap_err();

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

        let result = HostEnv::from_parts(&parts).unwrap();
        assert_eq!(result, HostEnv::new("root"));
    }

    #[test]
    fn it_from_parts_returns_error_when_missing() {
        let (parts, _) = Request::get("/")
            .body(HttpBody::empty())
            .unwrap()
            .into_parts();

        let err = HostEnv::from_parts(&parts).unwrap_err();
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

        let result = HostEnv::from_payload(Payload::Parts(&parts)).await.unwrap();
        assert_eq!(result, HostEnv::new("root"));
    }

    #[test]
    fn it_source_returns_parts_variant() {
        assert!(matches!(HostEnv::SOURCE, Source::Parts));
    }
}