use volga::headers::http_header;

#[http_header("x-api-key")]
pub struct ApiKey {
    pub inner: String,
}

fn main() {}
