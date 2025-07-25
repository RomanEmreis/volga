use std::ops::Add;
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use jsonwebtoken::{DecodingKey, EncodingKey};
use serde::{Deserialize, Serialize};
use volga::{ok, App};
use volga::headers::{AUTHORIZATION, CACHE_CONTROL, WWW_AUTHENTICATE};
use volga::auth::{Claims, BearerTokenService, predicate};

#[derive(Serialize, Deserialize)]
struct AuthData {
    access_token: String,
}

#[derive(Claims, Serialize, Deserialize)]
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
    tokio::spawn(async {
        let mut app = App::new()
            .bind("127.0.0.1:7960")
            .with_bearer_auth(|auth| auth
                .set_encoding_key(EncodingKey::from_secret(b"test secret"))
                .set_decoding_key(DecodingKey::from_secret(b"test secret"))
                .with_aud(["test"])
                .with_iss(["test"]));

        app.map_get("/test", || async { "Pass!" })
            .authorize(predicate(|claims: &Claims| claims.roles.iter().any(|role| role == "admin")));

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
        
        app.run().await
    });

    let token = tokio::spawn(async {
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().build().unwrap()
        };
        client.post("http://127.0.0.1:7960/login").send().await
    }).await.unwrap().unwrap().json::<AuthData>().await.unwrap().access_token;
    
    let response = tokio::spawn(async move {
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().build().unwrap()
        };
        client.get("http://127.0.0.1:7960/test").header(AUTHORIZATION, format!("Bearer {token}")).send().await
    }).await.unwrap().unwrap();

    assert!(response.status().is_success());
    assert_eq!(response.headers().get(CACHE_CONTROL).unwrap(), "no-store");
    assert_eq!(response.text().await.unwrap(), "Pass!");
}

#[tokio::test]
async fn it_generates_jwt_and_failed_to_authenticates_it() {
    tokio::spawn(async {
        let mut app = App::new()
            .bind("127.0.0.1:7961")
            .with_bearer_auth(|auth| auth
                .set_encoding_key(EncodingKey::from_secret(b"test secret"))
                .set_decoding_key(DecodingKey::from_secret(b"test secret"))
                .with_aud(["test"])
                .with_iss(["test"]));

        app.map_group("/tests")
            .authorize(predicate(|claims: &Claims| claims.roles.iter().any(|role| role == "admin")))
            .map_get("/test", || async { "Pass!" });

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

        app.run().await
    });

    let token = tokio::spawn(async {
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().build().unwrap()
        };
        client.post("http://127.0.0.1:7961/login").send().await
    }).await.unwrap().unwrap().json::<AuthData>().await.unwrap().access_token;

    let response = tokio::spawn(async move {
        let client = if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only().build().unwrap()
        } else {
            reqwest::Client::builder().http2_prior_knowledge().build().unwrap()
        };
        client.get("http://127.0.0.1:7961/tests/test").header(AUTHORIZATION, format!("Bearer {token}")).send().await
    }).await.unwrap().unwrap();

    assert!(!response.status().is_success());
    assert_eq!(response.headers().get(CACHE_CONTROL).unwrap(), "no-store");
    assert_eq!(response.headers().get(WWW_AUTHENTICATE).unwrap(), "Bearer error=\"insufficient_scope\" error_description=\"User does not have required role or permission\"");
}