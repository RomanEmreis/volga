//! Verifies `require_https` behavior: loopback exception and opt-out.

#![cfg(all(feature = "jwt-auth", feature = "test"))]

use serde::{Deserialize, Serialize};
use std::time::{SystemTime, UNIX_EPOCH};
use volga::{
    App,
    auth::{AuthClaims, BearerAuthConfig, BearerTokenService, DecodingKey, EncodingKey, roles},
    ok,
    test::TestServer,
};

const SECRET: &[u8] = b"test-require-https-secret-0123456";

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
    let service: BearerTokenService = BearerAuthConfig::default()
        .set_encoding_key(EncodingKey::from_secret(SECRET))
        .into();
    service.encode(&claims).unwrap().to_string()
}

#[tokio::test]
async fn it_allows_loopback_without_tls() {
    let server = TestServer::builder()
        .configure(|app| {
            app.with_bearer_auth(|auth| auth.set_decoding_key(DecodingKey::from_secret(SECRET)))
        })
        .setup(|app: &mut App| {
            app.map_get("/ok", || async { ok!("ok") })
                .authorize::<Claims>(roles(["admin"]));
        })
        .build()
        .await;

    let token = make_token();
    let res = server
        .client()
        .get(server.url("/ok"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200, "loopback should bypass require_https");
    server.shutdown().await;
}

#[tokio::test]
async fn it_respects_require_https_disabled() {
    let server = TestServer::builder()
        .configure(|app| {
            app.with_bearer_auth(|auth| {
                auth.set_decoding_key(DecodingKey::from_secret(SECRET))
                    .require_https(false)
            })
        })
        .setup(|app: &mut App| {
            app.map_get("/ok", || async { ok!("ok") })
                .authorize::<Claims>(roles(["admin"]));
        })
        .build()
        .await;

    let token = make_token();
    let res = server
        .client()
        .get(server.url("/ok"))
        .header("Authorization", format!("Bearer {token}"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    server.shutdown().await;
}
