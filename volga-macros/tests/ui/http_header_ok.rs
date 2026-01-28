#![allow(missing_docs)]

use volga::headers::{FromHeaders, HeaderMap, HeaderValue};
use volga_macros::http_header;

#[http_header("x-api-key")]
pub struct ApiKey;

fn main() {
    let mut headers = HeaderMap::new();
    headers.insert("x-api-key", HeaderValue::from_static("secret"));

    let _ = ApiKey::from_headers(&headers);
}
