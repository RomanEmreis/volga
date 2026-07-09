//! Verifies that WWW-Authenticate responses include `resource_metadata` when
//! configured.

#![cfg(all(feature = "jwt-auth", feature = "test"))]

use serde::{Deserialize, Serialize};
use volga::{
    App,
    auth::{AuthClaims, DecodingKey, roles},
    test::TestServer,
};

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

#[tokio::test]
async fn it_includes_resource_metadata_in_challenge() {
    let server = TestServer::builder()
        .configure(|app| {
            app.with_bearer_auth(|auth| {
                auth.set_decoding_key(DecodingKey::from_secret(b"wrong-secret"))
                    .with_resource_metadata_url(
                        "https://api.example.com/.well-known/oauth-protected-resource",
                    )
            })
        })
        .setup(|app: &mut App| {
            app.map_get("/x", || async { volga::ok!("x") })
                .authorize::<Claims>(roles(["admin"]));
        })
        .build()
        .await;

    let res = server
        .client()
        .get(server.url("/x"))
        .header("Authorization", "Bearer bad-token")
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 403);
    let www_auth = res.headers().get("www-authenticate").unwrap();
    let www_auth = www_auth.to_str().unwrap();
    assert!(
        www_auth.contains(
            r#"resource_metadata="https://api.example.com/.well-known/oauth-protected-resource""#
        ),
        "header was: {www_auth}"
    );
    server.shutdown().await;
}

#[tokio::test]
async fn it_derives_resource_metadata_from_mounted_metadata_route() {
    let server = TestServer::builder()
        .configure(|app| {
            app.with_bearer_auth(|auth| {
                auth.set_decoding_key(DecodingKey::from_secret(b"wrong-secret"))
            })
        })
        .setup(|app: &mut App| {
            app.use_oauth_resource_metadata("https://api.example.com");
            app.map_get("/x", || async { volga::ok!("x") })
                .authorize::<Claims>(roles(["admin"]));
        })
        .build()
        .await;

    let res = server
        .client()
        .get(server.url("/x"))
        .header("Authorization", "Bearer bad-token")
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 403);
    let www_auth = res.headers().get("www-authenticate").unwrap();
    let www_auth = www_auth.to_str().unwrap();
    assert!(
        www_auth.contains(
            r#"resource_metadata="https://api.example.com/.well-known/oauth-protected-resource""#
        ),
        "header was: {www_auth}"
    );
    server.shutdown().await;
}

#[tokio::test]
async fn it_prefers_explicit_resource_metadata_url_over_derived() {
    let server = TestServer::builder()
        .configure(|app| {
            app.with_bearer_auth(|auth| {
                auth.set_decoding_key(DecodingKey::from_secret(b"wrong-secret"))
                    .with_resource_metadata_url("https://api.example.com/custom-metadata")
            })
        })
        .setup(|app: &mut App| {
            app.use_oauth_resource_metadata("https://api.example.com");
            app.map_get("/x", || async { volga::ok!("x") })
                .authorize::<Claims>(roles(["admin"]));
        })
        .build()
        .await;

    let res = server
        .client()
        .get(server.url("/x"))
        .header("Authorization", "Bearer bad-token")
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 403);
    let www_auth = res.headers().get("www-authenticate").unwrap();
    let www_auth = www_auth.to_str().unwrap();
    assert!(
        www_auth.contains(r#"resource_metadata="https://api.example.com/custom-metadata""#),
        "header was: {www_auth}"
    );
    server.shutdown().await;
}
