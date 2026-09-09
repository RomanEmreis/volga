use volga::headers::http_header;

#[http_header(42)]
pub struct ApiKey;

fn main() {}
