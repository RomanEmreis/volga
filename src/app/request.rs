use http_body_util::BodyExt;
use std::collections::HashMap;
use std::sync::Arc;
use bytes::{Buf, Bytes};
use cancel::Cancel;
use http::{Request, Version};
use http::header::{HeaderName, HeaderValue};
use httparse::{Request as HttParseRequest, EMPTY_HEADER};
use serde::de::DeserializeOwned;
use tokio_util::sync::CancellationToken;
use tokio::io::Error;
use tokio::io::ErrorKind::{
    InvalidData,
    InvalidInput,
    UnexpectedEof
};
use crate::{Params, Payload};
use crate::app::body::{BoxBody, HttpBody};

pub mod params;
pub mod payload;
pub mod cancel;

pub type HttpRequest = Request<BoxBody>;
pub type RequestParams = Arc<HashMap<String, String>>;

pub(crate) struct RawRequest<'headers, 'buf> {
    raw_request: HttParseRequest<'headers, 'buf>,
    headers_size: usize
}

impl RawRequest<'_, '_> {
    #[inline]
    pub(crate) fn parse(buffer: &[u8]) -> Result<HttpRequest, Error> {
        let mut headers = [EMPTY_HEADER; 16];
        let mut req = HttParseRequest::new(&mut headers);
        
        match req.parse(buffer) {
            Ok(httparse::Status::Complete(headers_size)) => {
                Self::convert_to_http_request(buffer, RawRequest { 
                    raw_request: req,
                    headers_size
                })
            },
            Ok(httparse::Status::Partial) => Err(Error::new(UnexpectedEof, "Request is incomplete")),
            Err(e) => Err(Error::new(InvalidData, format!("Failed to parse request: {}", e)))
        }
    }

    #[inline]
    pub(crate) fn convert_to_http_request(buffer: &[u8], raw_req: RawRequest) -> Result<HttpRequest, Error> {
        let RawRequest {
            raw_request,
            headers_size
        } = raw_req;

        let method = raw_request.method.ok_or_else(|| Error::new(InvalidData, "No method specified"))?;
        let path = raw_request.path.ok_or_else(|| Error::new(InvalidData, "No path specified"))?;

        let mut builder = Request::builder()
            .method(method)
            .uri(path)
            .version(Version::HTTP_11); // assuming HTTP/1.1

        // Extract Content-Length, if present, to determine body length
        let content_length = raw_request.headers.iter()
            .find(|h| h.name.eq_ignore_ascii_case("Content-Length"))
            .and_then(|h| std::str::from_utf8(h.value).ok())
            .and_then(|s| s.parse::<usize>().ok())
            .unwrap_or(0);

        for header in raw_request.headers {
            let header_name = HeaderName::from_bytes(header.name.as_bytes())
                .map_err(|e| Error::new(InvalidData, format!("Invalid header name: {}", e)))?;

            let header_value = HeaderValue::from_bytes(header.value)
                .map_err(|e| Error::new(InvalidData, format!("Invalid header value: {}", e)))?;

            builder = builder.header(header_name, header_value);
        }

        let body = if content_length > 0 {
            let contend_end = headers_size + content_length;
            let bytes = Bytes::copy_from_slice(&buffer[headers_size..contend_end]);
            HttpBody::create(bytes)
        } else {
            HttpBody::empty()
        };

        let request = builder.body(body)
            .map_err(|_| Error::new(InvalidInput, "Failed to build request"))?;

        Ok(request)
    }
}

impl Payload for HttpRequest {
    #[inline]
    async fn payload<T>(self) -> Result<T, Error>
    where
        T: DeserializeOwned
    {
        let body = self.collect().await?.aggregate();
        let data: T = serde_json::from_reader(body.reader())?;
        Ok(data)
    }
}

impl Params for HttpRequest {
    #[inline]
    fn params(&self) -> Option<&RequestParams> {
        self.extensions().get::<RequestParams>()
    }

    #[inline]
    fn param(&self, name: &str) -> Result<&String, Error> {
        self.params()
            .and_then(|params| params.get(name))
            .ok_or(Error::new(InvalidInput, format!("Missing parameter: {name}")))
    }
}

impl Cancel for HttpRequest {
    fn cancellation_token(&self) -> CancellationToken {
        if let Some(token) = self.extensions().get::<CancellationToken>() {
            token.clone()
        } else { 
            CancellationToken::new()
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{collections::HashMap, sync::Arc};
    use serde::Deserialize;
    use tokio_util::sync::CancellationToken;
    use crate::{Cancel, Params, Payload};
    use crate::app::body::HttpBody;
    use super::HttpRequest;

    #[derive(Deserialize)]
    struct TestPayload {
        name: String
    }

    #[tokio::test]
    async fn it_parses_payload() {
        let request_body = "{\"name\":\"test\"}";
        let request = HttpRequest::new(HttpBody::full(request_body));

        let payload: TestPayload = request.payload().await.unwrap();

        assert_eq!(payload.name, "test");
    }

    #[test]
    fn it_reads_params() {
        let mut params = HashMap::new();
        params.insert(String::from("name"), String::from("test"));

        let mut request = HttpRequest::new(HttpBody::empty());
        request.extensions_mut().insert(Arc::new(params));

        let request_params = request.params().unwrap();

        assert_eq!(request_params.len(), 1);

        let name = request_params.get("name").unwrap();

        assert_eq!(name, "test");
    }

    #[test]
    fn it_reads_param() {
        let mut params = HashMap::new();
        params.insert(String::from("name"), String::from("test"));

        let mut request = HttpRequest::new(HttpBody::empty());
        request.extensions_mut().insert(Arc::new(params));

        let name = request.param("name").unwrap();

        assert_eq!(name, "test");
    }

    #[test]
    fn it_cancels() {
        let token = CancellationToken::new();

        let mut request = HttpRequest::new(HttpBody::empty());
        request.extensions_mut().insert(token.clone());

        let req_token = request.cancellation_token();
        req_token.cancel();

        assert!(token.is_cancelled());
    }

}
