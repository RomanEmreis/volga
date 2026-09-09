use volga::headers::http_header;

#[http_header("x-api-key")]
pub enum ApiKey {
    Missing,
}

fn main() {}
