//! Extractors for the whole HTTP request

use crate::{error::Error, HttpRequest};
use futures_util::future::{ok, Ready};

use hyper::{
    http::{Extensions, request::Parts},
    Method, 
    Uri
};

use crate::headers::{HeaderMap, HeaderValue};
use crate::http::endpoints::args::{
    FromRequestParts,
    FromRequestRef,
    FromPayload,
    Payload,
    Source
};

impl FromRequestParts for Parts {
    #[inline]
    fn from_parts(parts: &Parts) -> Result<Self, Error> {
        Ok(parts.clone())
    }
}

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

impl FromRequestParts for Extensions {
    #[inline]
    fn from_parts(parts: &Parts) -> Result<Self, Error> {
        Ok(parts.extensions.clone())
    }
}

impl FromRequestRef for Extensions {
    #[inline]
    fn from_request(req: &HttpRequest) -> Result<Self, Error> {
        Ok(req.extensions().clone())
    }
}

impl FromRequestParts for HeaderMap<HeaderValue> {
    #[inline]
    fn from_parts(parts: &Parts) -> Result<Self, Error> {
        Ok(parts.headers.clone())
    }
}

impl FromRequestRef for HeaderMap<HeaderValue> {
    #[inline]
    fn from_request(req: &HttpRequest) -> Result<Self, Error> {
        Ok(req.headers().clone())
    }
}

impl FromPayload for HttpRequest {
    type Future = Ready<Result<Self, Error>>;

    #[inline]
    fn from_payload(payload: Payload) -> Self::Future {
        let Payload::Request(req) = payload else { unreachable!() };
        ok(*req)
    }

    fn source() -> Source {
        Source::Request
    }
}

#[cfg(test)]
mod tests {
    use hyper::http::request::Parts;
    use super::{FromRequestParts, FromPayload, Payload};
    use hyper::{HeaderMap, Method, Request, Uri};
    use hyper::http::Extensions;
    use crate::error::Error;
    use crate::headers::HeaderValue;
    use crate::{HttpBody, HttpRequest};

    #[test]
    fn it_gets_parts_clone_from_parts() {
        let req = Request::get("/").body(()).unwrap();
        let (parts, _) = req.into_parts();
        
        let parts = Parts::from_parts(&parts);
        
        assert!(parts.is_ok());
        
        let parts = parts.unwrap();
        assert_eq!(parts.uri.path(), "/");
    }

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

    #[test]
    fn it_gets_headers_from_parts() {
        let req = Request::get("/")
            .header("header", "val")
            .body(())
            .unwrap();
        
        let (parts, _) = req.into_parts();

        let headers = HeaderMap::<HeaderValue>::from_parts(&parts);

        assert!(headers.is_ok());

        let headers = headers.unwrap();
        assert_eq!(headers.get("header"), Some(&HeaderValue::from_static("val")));
    }

    #[test]
    fn it_gets_extensions_from_parts() {
        let req = Request::get("/")
            .extension("ext")
            .body(())
            .unwrap();

        let (parts, _) = req.into_parts();

        let ext = Extensions::from_parts(&parts);

        assert!(ext.is_ok());

        let ext = ext.unwrap();
        assert_eq!(ext.get::<&str>().cloned(), Some("ext"));
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
}