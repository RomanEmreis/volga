use volga::{error::Error, http::{FromRequestRef, RequestHead}};

struct Custom;

impl FromRequestRef for Custom {
    fn from_request(_: &RequestHead<'_>) -> Result<Self, Error> {
        Ok(Custom)
    }
}

fn main() {}
