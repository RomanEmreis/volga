//! The status a rejected request gets, per RFC 6750 Section 3.1.
//!
//! The distinction matters to clients: 401 with `invalid_token` tells one holding a
//! stale credential to refresh it, while 403 says the credential is fine and refreshing
//! it will not help.

#![cfg(all(feature = "jwt-auth", feature = "test"))]
#![allow(missing_docs)]

use jsonwebtoken::{EncodingKey, Header, encode};
use serde::{Deserialize, Serialize};
use volga::{
    App,
    auth::{AuthClaims, DecodingKey, roles},
    test::TestServer,
};

const SECRET: &[u8] = b"test secret";
const FAR_FUTURE: u64 = 4_102_444_800; // 2100-01-01

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

fn token(role: &str, exp: u64) -> String {
    let claims = Claims {
        sub: "someone".into(),
        role: role.into(),
        exp,
    };
    encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(SECRET),
    )
    .expect("failed to sign the token")
}

async fn server() -> TestServer {
    TestServer::builder()
        .configure(|app| {
            app.with_bearer_auth(|auth| auth.set_decoding_key(DecodingKey::from_secret(SECRET)))
        })
        .setup(|app: &mut App| {
            app.map_get("/x", || async { volga::ok!("x") })
                .authorize::<Claims>(roles(["admin"]));
        })
        .build()
        .await
}

/// Sends a request with the given `Authorization` header, if any, and reports the status
/// alongside the `WWW-Authenticate` challenge.
async fn call(server: &TestServer, authorization: Option<&str>) -> (u16, String) {
    let mut request = server.client().get(server.url("/x"));
    if let Some(value) = authorization {
        request = request.header("Authorization", value);
    }
    let response = request.send().await.expect("the request failed");
    let status = response.status().as_u16();
    let challenge = response
        .headers()
        .get("www-authenticate")
        .and_then(|v| v.to_str().ok())
        .unwrap_or_default()
        .to_string();
    (status, challenge)
}

#[tokio::test]
async fn it_accepts_a_token_carrying_the_required_role() {
    let server = server().await;
    let bearer = format!("Bearer {}", token("admin", FAR_FUTURE));

    assert_eq!(call(&server, Some(&bearer)).await.0, 200);

    server.shutdown().await;
}

#[tokio::test]
async fn it_challenges_without_an_error_code_when_no_credential_is_sent() {
    let server = server().await;

    // Nothing was presented, so there is nothing to call invalid: a bare challenge lets
    // the client discover the resource metadata and start a flow
    let (status, challenge) = call(&server, None).await;
    assert_eq!(status, 401);
    assert!(
        challenge.starts_with("Bearer"),
        "challenge was: {challenge}"
    );
    assert!(!challenge.contains("error="), "challenge was: {challenge}");

    server.shutdown().await;
}

#[tokio::test]
async fn it_rejects_a_credential_that_is_not_a_bearer_value() {
    let server = server().await;

    // The header itself is wrong, not the token - the client has to fix the request
    let (status, challenge) = call(&server, Some("Basic dXNlcjpwYXNz")).await;
    assert_eq!(status, 400);
    assert!(
        challenge.contains(r#"error="invalid_request""#),
        "challenge was: {challenge}"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_rejects_a_malformed_token_as_unauthorized() {
    let server = server().await;

    let (status, challenge) = call(&server, Some("Bearer not-a-token")).await;
    assert_eq!(status, 401);
    assert!(
        challenge.contains(r#"error="invalid_token""#),
        "challenge was: {challenge}"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_rejects_a_token_that_does_not_decode_as_unauthorized() {
    let server = server().await;

    // The header carries a bearer credential, and what it carries is not a token: the first
    // segment is not base64 at all. RFC 6750 Section 3.1 calls that a malformed access token
    // - `invalid_token` and `401` - and keeps `invalid_request` for a request that is wrong
    // about how it carries the token rather than about the token
    let (status, challenge) = call(&server, Some("Bearer @@@.aGVsbG8.c2ln")).await;
    assert_eq!(status, 401);
    assert!(
        challenge.contains(r#"error="invalid_token""#),
        "challenge was: {challenge}"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_rejects_a_token_signed_with_the_wrong_key_as_unauthorized() {
    let server = server().await;
    let wrong = encode(
        &Header::default(),
        &Claims {
            sub: "someone".into(),
            role: "admin".into(),
            exp: FAR_FUTURE,
        },
        &EncodingKey::from_secret(b"another secret"),
    )
    .expect("failed to sign the token");

    let (status, challenge) = call(&server, Some(&format!("Bearer {wrong}"))).await;
    assert_eq!(status, 401);
    assert!(
        challenge.contains(r#"error="invalid_token""#),
        "challenge was: {challenge}"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_rejects_an_expired_token_as_unauthorized() {
    let server = server().await;
    let bearer = format!("Bearer {}", token("admin", 1));

    // A refresh fixes this one, so the client has to be told to try
    let (status, challenge) = call(&server, Some(&bearer)).await;
    assert_eq!(status, 401);
    assert!(
        challenge.contains(r#"error="invalid_token""#),
        "challenge was: {challenge}"
    );

    server.shutdown().await;
}

#[tokio::test]
async fn it_forbids_a_valid_token_that_lacks_the_required_role() {
    let server = server().await;
    let bearer = format!("Bearer {}", token("guest", FAR_FUTURE));

    // The credential is in order, it just does not carry enough authority. Refreshing it
    // would change nothing, so this one stays a 403
    let (status, challenge) = call(&server, Some(&bearer)).await;
    assert_eq!(status, 403);
    assert!(
        challenge.contains(r#"error="insufficient_scope""#),
        "challenge was: {challenge}"
    );

    server.shutdown().await;
}
