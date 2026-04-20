//! Verifies that the Authorization header is removed from the request after
//! successful bearer-token auth, unless disabled.

#![cfg(all(feature = "jwt-auth", feature = "test"))]

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use volga::{
    HttpRequest,
    auth::{AuthClaims, DecodingKey, EncodingKey, roles},
    ok,
    test::TestServer,
};

const SECRET: &[u8] = b"test-strip-token-secret-0123456789";

#[derive(Clone, Debug, Serialize, Deserialize)]
struct Claims {
    sub: String,
    role: String,
    exp: u64,
}

impl AuthClaims for Claims {
    fn role(&self) -> Option<&str> {
        Some(&self.role)
    }
}

fn make_token() -> String {
    let claims = Claims {
        sub: "alice".to_string(),
        role: "admin".to_string(),
        exp: SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
            + 3600,
    };
    let key = EncodingKey::from_secret(SECRET);
    let service: volga::auth::BearerTokenService = volga::auth::BearerAuthConfig::default()
        .set_encoding_key(key)
        .validate_exp(false)
        .into();
    service.encode(&claims).unwrap().to_string()
}

async fn echo_auth_header(req: HttpRequest) -> volga::HttpResult {
    if req.headers().get(hyper::header::AUTHORIZATION).is_some() {
        ok!("present")
    } else {
        ok!("absent")
    }
}

#[tokio::test]
async fn it_strips_authorization_header_after_auth() {
    let server = TestServer::builder()
        .configure(|app| {
            app.with_bearer_auth(|auth| {
                auth.set_decoding_key(DecodingKey::from_secret(SECRET))
                    .validate_exp(false)
            })
        })
        .setup(|app| {
            app.map_get("/echo", echo_auth_header)
                .authorize::<Claims>(roles(["admin"]));
        })
        .build()
        .await;

    let token = make_token();
    let res = server
        .client()
        .get(server.url("/echo"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(res.text().await.unwrap(), "absent");
    server.shutdown().await;
}

#[tokio::test]
async fn it_preserves_authorization_header_when_disabled() {
    let server = TestServer::builder()
        .configure(|app| {
            app.with_bearer_auth(|auth| {
                auth.set_decoding_key(DecodingKey::from_secret(SECRET))
                    .validate_exp(false)
                    .strip_token_from_request(false)
            })
        })
        .setup(|app| {
            app.map_get("/echo", echo_auth_header)
                .authorize::<Claims>(roles(["admin"]));
        })
        .build()
        .await;

    let token = make_token();
    let res = server
        .client()
        .get(server.url("/echo"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    assert_eq!(res.text().await.unwrap(), "present");
    server.shutdown().await;
}
