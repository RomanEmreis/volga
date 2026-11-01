use volga::{HttpRequest, error::Error, http::FromRequestRef};

struct Custom;

impl FromRequestRef for Custom {
    fn from_request(_: &HttpRequest) -> Result<Self, Error> {
        Ok(Custom)
    }
}

fn main() {}
