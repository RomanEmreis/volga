use std::collections::HashMap;
use std::io::Error;

use futures_util::future::{ready, Ready};

use hyper::{
    http::request::Parts,
    http::HeaderValue,
    HeaderMap
};

use crate::app::endpoints::args::{FromPayload, Payload, PayloadType};

pub struct Headers {
    inner: HashMap<String, String>
}

impl Headers {
    pub fn into_inner(self) -> HashMap<String, String> {
        self.inner
    }
    
    pub(super) fn from_headers_map(headers: &HeaderMap<HeaderValue>) -> Result<Self, Error> {
        let headers: HashMap<String, String> = headers
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_str().unwrap().to_string()))
            .collect();

        Ok(Headers { inner: headers })
    }
}

impl FromPayload for Headers {
    type Future = Ready<Result<Self, Error>>;

    #[inline]
    fn from_payload(req: &Parts, _payload: Payload) -> Self::Future {
        let headers = Self::from_headers_map(&req.headers);
        ready(headers)
    }

    #[inline]
    fn payload_type() -> PayloadType {
        PayloadType::Headers
    }
}

#[cfg(test)]
mod tests {
    use hyper::Request;
    use crate::Headers;

    #[test]
    fn it_parses_headers() {
        let request = Request::get("http://localhost:8000/test")
            .header("User-Agent", "Mozilla/5.0")
            .header("accept-encoding", "gzip")
            .body(())
            .unwrap();
        
        let headers = Headers::from_headers_map(request.headers())
            .unwrap()
            .into_inner();
        
        assert_eq!(headers.len(), 2);
        assert_eq!(headers.get("user-agent").unwrap(), "Mozilla/5.0");
        assert_eq!(headers.get("accept-encoding").unwrap(), "gzip");
    }
}
