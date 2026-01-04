#![allow(missing_docs)]
#![cfg(all(feature = "test", feature = "middleware", feature = "jwt-auth-full"))]

use std::ops::Add;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use jsonwebtoken::{DecodingKey, EncodingKey};
use serde::{Deserialize, Serialize};
use volga::{ok, headers::{ CACHE_CONTROL, WWW_AUTHENTICATE}};
use volga::auth::{Claims, BearerTokenService, predicate};
use volga::test::TestServer;

#[derive(Serialize, Deserialize)]
struct AuthData {
    access_token: String,
}

#[derive(Claims, Clone, Serialize, Deserialize)]
struct Claims {
    sub: String,
    iss: String,
    aud: String,
    company: String,
    roles: Vec<String>,
    permissions: Vec<String>,
    exp: u64,
}

#[tokio::test]
async fn it_generates_jwt_and_authenticates_it() {
    let server = TestServer::builder()
        .with_app(|app| app
            .with_bearer_auth(|auth| auth
                .set_encoding_key(EncodingKey::from_secret(b"test secret"))
                .set_decoding_key(DecodingKey::from_secret(b"test secret"))
                .with_aud(["test"])
                .with_iss(["test"])))
        .setup(|app| {
            app.map_get("/test", || async { "Pass!" })
                .authorize(predicate(|claims: &Claims| claims.roles.iter().any(|role| role == "admin")));
        })
        .setup(|app| {
            app.map_post("/login", |bts: BearerTokenService| async move {
                let exp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("Time went backwards")
                    .add(Duration::from_secs(300))
                    .as_secs();

                let claims = Claims {
                    aud: "test".to_owned(),
                    iss: "test".to_owned(),
                    sub: "email.com".to_owned(),
                    company: "Awesome Co.".to_owned(),
                    roles: vec!["admin".to_owned()],
                    permissions: vec![],
                    exp,
                };

                let access_token = bts.encode(&claims)?
                    .to_string();

                ok!(AuthData { access_token })
            });
        })
        .build()
        .await;
    
    let token = server.client()
        .post(server.url("/login"))
        .send()
        .await
        .unwrap()
        .json::<AuthData>()
        .await
        .unwrap()
        .access_token;

    let response = server.client()
        .get(server.url("/test"))
        .bearer_auth(token)
        .send()
        .await
        .unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.headers().get(CACHE_CONTROL).unwrap(), "no-store");
    assert_eq!(response.text().await.unwrap(), "Pass!");
    
    server.shutdown().await;
}

#[tokio::test]
async fn it_generates_jwt_and_failed_to_authenticates_it() {
    let server = TestServer::builder()
        .with_app(|app| app
            .with_bearer_auth(|auth| auth
                .set_encoding_key(EncodingKey::from_secret(b"test secret"))
                .set_decoding_key(DecodingKey::from_secret(b"test secret"))
                .with_aud(["test"])
                .with_iss(["test"])))
        .setup(|app| {
            app.map_get("/test", || async { "Pass!" })
                .authorize(predicate(|claims: &Claims| claims.roles.iter().any(|role| role == "admin")));
        })
        .setup(|app| {
            app.map_post("/login", |bts: BearerTokenService| async move {
                let exp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .expect("Time went backwards")
                    .add(Duration::from_secs(300))
                    .as_secs();

                let claims = Claims {
                    aud: "test".to_owned(),
                    iss: "test".to_owned(),
                    sub: "email.com".to_owned(),
                    company: "Awesome Co.".to_owned(),
                    roles: vec!["user".to_owned()],
                    permissions: vec![],
                    exp,
                };

                let access_token = bts.encode(&claims)?
                    .to_string();

                ok!(AuthData { access_token })
            });
        })
        .build()
        .await;

    let token = server.client()
        .post(server.url("/login"))
        .send()
        .await
        .unwrap()
        .json::<AuthData>()
        .await
        .unwrap()
        .access_token;

    let response = server.client()
        .get(server.url("/test"))
        .bearer_auth(token)
        .send()
        .await
        .unwrap();

    assert!(!response.status().is_success());
    assert_eq!(response.headers().get(CACHE_CONTROL).unwrap(), "no-store");
    assert_eq!(
        response.headers().get(WWW_AUTHENTICATE).unwrap(), 
        "Bearer error=\"insufficient_scope\" error_description=\"User does not have required role or permission\""
    );
    
    server.shutdown().await;
}