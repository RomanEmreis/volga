use volga::auth::Claims;

#[derive(Claims)]
union AccessClaims {
    role: u32,
}

fn main() {}
