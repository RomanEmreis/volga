#![allow(missing_docs)]

use volga_macros::http_header;

#[http_header(123)]
pub struct ApiKey;

fn main() {}
