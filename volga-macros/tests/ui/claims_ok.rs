#![allow(missing_docs)]

use serde::Deserialize;
use volga_macros::Claims;

#[derive(Clone, Deserialize, Claims)]
struct MyClaims {
    sub: String,
    role: String,
    roles: Vec<String>,
    permissions: Vec<String>,
}

fn main() {}
