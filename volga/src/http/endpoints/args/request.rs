//! Extractors for the whole HTTP request

use crate::{error::Error, HttpRequest};
use futures_util::future::{ok, Ready};

use hyper::{
    http::request::Parts,
    Method, 
    Uri
};

use crate::http::endpoints::args::{
    FromRequestParts,
    FromRequestRef,
    FromPayload,
    Payload,
    Source
};

impl FromRequestParts for Uri {
    #[inline]
    fn from_parts(parts: &Parts) -> Result<Self, Error> {
        Ok(parts.uri.clone())
    }
}

impl FromRequestRef for Uri {
    #[inline]
    fn from_request(req: &HttpRequest) -> Result<Self, Error> {
        Ok(req.uri().clone())
    }
}

impl FromRequestParts for Method {
    #[inline]
    fn from_parts(parts: &Parts) -> Result<Self, Error> {
        Ok(parts.method.clone())
    }
}

impl FromRequestRef for Method {
    #[inline]
    fn from_request(req: &HttpRequest) -> Result<Self, Error> {
        Ok(req.method().clone())
    }
}

impl FromPayload for Uri {
    type Future = Ready<Result<Self, Error>>;

    const SOURCE: Source = Source::Parts;
    
    #[inline]
    fn from_payload(payload: Payload<'_>) -> Self::Future {
        let Payload::Parts(parts) = payload else { unreachable!() };
        ok(parts.uri.clone())
    }
}

impl FromPayload for Method {
    type Future = Ready<Result<Self, Error>>;

    const SOURCE: Source = Source::Parts;
    
    #[inline]
    fn from_payload(payload: Payload<'_>) -> Self::Future {
        let Payload::Parts(parts) = payload else { unreachable!() };
        ok(parts.method.clone())
    }
}

impl FromPayload for HttpRequest {
    type Future = Ready<Result<Self, Error>>;

    const SOURCE: Source = Source::Request;
    
    #[inline]
    fn from_payload(payload: Payload<'_>) -> Self::Future {
        let Payload::Request(req) = payload else { unreachable!() };
        ok(*req)
    }
}

#[cfg(test)]
mod tests {
    use super::{FromRequestParts, FromPayload, Payload};
    use hyper::{Method, Request, Uri};
    use crate::error::Error;
    use crate::{HttpBody, HttpRequest};

    #[test]
    fn it_gets_uri_from_parts() {
        let req = Request::get("/").body(()).unwrap();
        let (parts, _) = req.into_parts();

        let uri: Result<Uri, Error> = FromRequestParts::from_parts(&parts);

        assert!(uri.is_ok());

        let uri = uri.unwrap();
        assert_eq!(uri.path(), "/");
    }

    #[test]
    fn it_gets_method_from_parts() {
        let req = Request::get("/").body(()).unwrap();
        let (parts, _) = req.into_parts();

        let method = Method::from_parts(&parts);

        assert!(method.is_ok());

        let method = method.unwrap();
        assert_eq!(method, Method::GET);
    }

    #[tokio::test]
    async fn it_gets_http_req_from_parts() {
        let req = Request::get("/").body(HttpBody::empty()).unwrap();
        let (parts, body) = req.into_parts();
        let req = HttpRequest::from_parts(parts, body);

        let req = HttpRequest::from_payload(Payload::Request(Box::new(req))).await;

        assert!(req.is_ok());

        let req = req.unwrap();
        
        assert_eq!(req.uri().path(), "/");
        assert_eq!(req.method(), Method::GET);
    }

    #[tokio::test]
    async fn it_gets_method_from_payload() {
        let req = Request::get("/")
            .body(())
            .unwrap();

        let (parts, _) = req.into_parts();

        let method = Method::from_payload(Payload::Parts(&parts)).await;

        assert!(method.is_ok());

        let method = method.unwrap();
        assert_eq!(method, Method::GET);
    }

    #[tokio::test]
    async fn it_gets_uri_from_payload() {
        let req = Request::get("/")
            .body(())
            .unwrap();

        let (parts, _) = req.into_parts();

        let uri = Uri::from_payload(Payload::Parts(&parts)).await;

        assert!(uri.is_ok());

        let uri = uri.unwrap();
        assert_eq!(uri.path(), "/");
    }
}