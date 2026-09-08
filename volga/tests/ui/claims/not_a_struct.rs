use volga::auth::Claims;

#[derive(Claims)]
enum AccessClaims {
    Anonymous,
}

fn main() {}
