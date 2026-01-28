#![allow(missing_docs)]

use volga_macros::Claims;

#[derive(Claims)]
struct BadClaims {
    sub: String,
}

fn main() {}
