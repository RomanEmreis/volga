//! Run with:
//!
//! ```no_rust
//! JWT_SECRET=secret cargo run -p jwt
//! ```

use std::{ops::Add, time::{SystemTime, UNIX_EPOCH, Duration}};
use serde::{Serialize, Deserialize};
use volga::{
    App, Json, HttpResult,
    auth::{
        Authenticated,
        BearerTokenService, 
        DecodingKey, 
        EncodingKey,
        Claims,
        roles
    },
    ok, status, bad_request
};

fn main() {
    let secret = std::env::var("JWT_SECRET")
        .expect("JWT_SECRET must be set");

    let mut app = App::new()
        .with_bearer_auth(|auth| auth
            .set_encoding_key(EncodingKey::from_secret(secret.as_bytes()))
            .set_decoding_key(DecodingKey::from_secret(secret.as_bytes())));

    app.map_post("/login", login);
    app.map_get("/me", me)
        .authorize::<Claims>(roles(["admin", "user"]));

    app.run_blocking()
}

async fn login(payload: Json<Payload>, bts: BearerTokenService) -> HttpResult {
    if payload.client_id.is_empty() || payload.client_secret.is_empty() {
        return bad_request!("Missing Credentials");
    }
    if payload.client_id != "foo" || payload.client_secret != "bar" {
        return status!(401, "Invalid Credentials");
    }

    let exp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("Time went backwards")
        .add(Duration::from_secs(300))
        .as_secs();

    let claims = Claims {
        sub: "email.com".to_owned(),
        company: "Awesome Co.".to_owned(),
        role: "admin".to_owned(),
        exp,
    };

    let access_token = bts.encode(&claims)?
        .to_string();

    ok!(AuthData { access_token })
}

async fn me(auth: Authenticated<Claims>) -> String {
    format!("Hello from protected area {}!", auth.sub)
}

#[derive(Claims, Clone, Serialize, Deserialize)]
struct Claims {
    sub: String,
    company: String,
    role: String,
    exp: u64,
}

#[derive(Serialize)]
struct AuthData {
    access_token: String,
}

#[derive(Deserialize)]
struct Payload {
    client_id: String,
    client_secret: String,
}