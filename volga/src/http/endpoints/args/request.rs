//! Extractors for the whole HTTP request

use crate::{HttpBody, HttpRequest, error::Error};
use futures_util::future::{Ready, ok};

use hyper::{Method, Uri};

use crate::http::endpoints::args::{
    FromPayload, FromRequestHead, Payload, RequestHead, Source, sealed::PathAccess,
};

impl FromRequestHead for Uri {
    #[inline]
    fn from_head(head: &RequestHead<'_>) -> Result<Self, Error> {
        Ok(head.uri().clone())
    }

    // Reads no path parameter, so a positional extractor next to it takes its own
    const PATH_ACCESS: PathAccess = PathAccess::Ignored;
}

impl FromRequestHead for Method {
    #[inline]
    fn from_head(head: &RequestHead<'_>) -> Result<Self, Error> {
        Ok(head.method().clone())
    }

    // Reads no path parameter, so a positional extractor next to it takes its own
    const PATH_ACCESS: PathAccess = PathAccess::Ignored;
}

impl FromPayload for HttpRequest {
    type Future = Ready<Result<Self, Error>>;

    const SOURCE: Source = Source::Request;

    #[inline]
    fn from_payload(payload: Payload<'_>) -> Self::Future {
        let Payload::Request(req) = payload else {
            unreachable!()
        };
        ok(req)
    }
}

impl FromPayload for HttpBody {
    type Future = Ready<Result<Self, Error>>;

    const SOURCE: Source = Source::Body;

    #[inline]
    fn from_payload(payload: Payload<'_>) -> Self::Future {
        let Payload::Body(body) = payload else {
            unreachable!()
        };
        ok(body)
    }
}

#[cfg(test)]
mod tests {
    use super::{FromPayload, Payload};
    use crate::error::Error;
    use crate::http::FromRequestRef;
    use crate::http::RequestHead;
    use crate::{HttpBody, HttpRequest};
    use http_body_util::BodyExt;
    use hyper::{Method, Request, Uri};

    #[test]
    fn it_gets_uri_from_parts() {
        let req = Request::get("/").body(()).unwrap();
        let (parts, _) = req.into_parts();

        let uri: Result<Uri, Error> =
            FromRequestRef::from_request(&RequestHead::from_parts(&parts));

        assert!(uri.is_ok());

        let uri = uri.unwrap();
        assert_eq!(uri.path(), "/");
    }

    #[test]
    fn it_gets_method_from_parts() {
        let req = Request::get("/").body(()).unwrap();
        let (parts, _) = req.into_parts();

        let method = Method::from_request(&RequestHead::from_parts(&parts));

        assert!(method.is_ok());

        let method = method.unwrap();
        assert_eq!(method, Method::GET);
    }

    #[tokio::test]
    async fn it_gets_http_req_from_payload() {
        let req = Request::get("/").body(HttpBody::empty()).unwrap();
        let (parts, body) = req.into_parts();
        let req = HttpRequest::from_parts(parts, body);

        let req = HttpRequest::from_payload(Payload::Request(req)).await;

        assert!(req.is_ok());

        let req = req.unwrap();

        assert_eq!(req.uri().path(), "/");
        assert_eq!(req.method(), Method::GET);
    }

    #[tokio::test]
    async fn it_gets_method_from_payload() {
        let req = Request::get("/").body(()).unwrap();

        let (parts, _) = req.into_parts();

        let method = Method::from_payload(Payload::Parts(RequestHead::from_parts(&parts))).await;

        assert!(method.is_ok());

        let method = method.unwrap();
        assert_eq!(method, Method::GET);
    }

    #[tokio::test]
    async fn it_gets_uri_from_payload() {
        let req = Request::get("/").body(()).unwrap();

        let (parts, _) = req.into_parts();

        let uri = Uri::from_payload(Payload::Parts(RequestHead::from_parts(&parts))).await;

        assert!(uri.is_ok());

        let uri = uri.unwrap();
        assert_eq!(uri.path(), "/");
    }

    #[tokio::test]
    async fn it_gets_http_body_from_payload() {
        let req = Request::get("/").body(HttpBody::full("test")).unwrap();
        let (_, body) = req.into_parts();

        let req = HttpBody::from_payload(Payload::Body(body)).await;

        assert!(req.is_ok());

        let bytes = req.unwrap().collect().await.unwrap().to_bytes();

        assert_eq!(String::from_utf8_lossy(&bytes), "test");
    }
}
