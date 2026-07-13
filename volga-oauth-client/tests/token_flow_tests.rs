//! End-to-end token flow tests: a real volga application playing the
//! token endpoint for [`OAuthClient`]. Same HTTP/1-only gating as the
//! discovery suite (see `discovery_tests.rs`).

#![cfg(feature = "http1")]

mod common;

use std::{
    sync::Arc,
    time::{Duration, SystemTime},
};

use common::{free_port, serve};
use serde::Deserialize;
use volga::{App, Form, HttpRequest};
use volga_oauth_client::{
    AuthorizationServerMetadata, ClientConfig, ClientError, InMemoryTokenStore, OAuthClient,
    OAuthErrorCode, TokenSet, TokenStore,
};

/// The fields our fake token endpoint cares about.
#[derive(Deserialize)]
struct TokenForm {
    grant_type: String,
    code: Option<String>,
    code_verifier: Option<String>,
    redirect_uri: Option<String>,
    refresh_token: Option<String>,
    resource: Option<String>,
    client_id: Option<String>,
}

/// An OAuth client accepting the plaintext test server.
fn plaintext_client(client_id: &str) -> OAuthClient {
    OAuthClient::new(client_id).with_config(ClientConfig::new().require_https(false))
}

fn server_metadata(base: &str) -> AuthorizationServerMetadata {
    let mut metadata = AuthorizationServerMetadata::new(base);
    metadata.authorization_endpoint = Some(format!("{base}/authorize"));
    metadata.token_endpoint = Some(format!("{base}/token"));
    metadata
}

fn stored(access_token: &str, refresh_token: Option<&str>, expires_at: SystemTime) -> TokenSet {
    TokenSet {
        access_token: access_token.into(),
        token_type: "Bearer".into(),
        refresh_token: refresh_token.map(Into::into),
        scope: None,
        id_token: None,
        expires_at: Some(expires_at),
    }
}

#[tokio::test]
async fn it_exchanges_an_authorization_code_for_tokens() {
    let port = free_port();
    let base = format!("http://127.0.0.1:{port}");

    // the endpoint echoes the received verifier and resource back through
    // the token response, so the client side can assert both went through
    let mut app = App::new();
    app.map_post("/token", |form: Form<TokenForm>| async move {
        if form.grant_type != "authorization_code"
            || form.code.as_deref() != Some("c0de")
            || form.client_id.as_deref() != Some("my-client")
            || form.redirect_uri.as_deref() != Some("https://app.example.com/callback")
        {
            return volga::status!(400, { "error": "invalid_request" });
        }
        volga::ok!({
            "access_token": form.code_verifier.clone().unwrap_or_default(),
            "token_type": "Bearer",
            "expires_in": 3600,
            "refresh_token": "r1",
            "scope": form.resource.clone().unwrap_or_default()
        })
    });
    let server = serve(port, app).await;

    let metadata = server_metadata(&base);
    let client =
        plaintext_client("my-client").with_redirect_uri("https://app.example.com/callback");
    let auth = client
        .authorization_request(&metadata)
        .with_scopes(["read"])
        .with_resource("https://api.example.com")
        .build()
        .unwrap();

    let tokens = client
        .exchange_code(&metadata, "c0de", &auth)
        .await
        .unwrap();
    assert_eq!(tokens.access_token, auth.pkce.verifier());
    assert_eq!(tokens.scope.as_deref(), Some("https://api.example.com"));
    assert_eq!(tokens.refresh_token.as_deref(), Some("r1"));
    assert!(!tokens.is_expired());
    server.abort();
}

#[tokio::test]
async fn it_authenticates_with_client_secret_basic() {
    let port = free_port();
    let base = format!("http://127.0.0.1:{port}");

    // base64("confidential:s3cret")
    let mut app = App::new();
    app.map_post("/token", |req: HttpRequest| async move {
        match req.headers().get(hyper::header::AUTHORIZATION) {
            Some(value) if value == "Basic Y29uZmlkZW50aWFsOnMzY3JldA==" => {
                volga::ok!({ "access_token": "at", "token_type": "Bearer" })
            }
            _ => volga::status!(401, { "error": "invalid_client" }),
        }
    });
    let server = serve(port, app).await;

    let metadata = server_metadata(&base);
    let tokens = plaintext_client("confidential")
        .with_secret("s3cret")
        .refresh(&metadata, "r1")
        .await
        .unwrap();
    assert_eq!(tokens.access_token, "at");

    let err = plaintext_client("confidential")
        .with_secret("wrong")
        .refresh(&metadata, "r1")
        .await
        .unwrap_err();
    assert!(
        matches!(
            &err,
            ClientError::Protocol(oauth) if oauth.error == OAuthErrorCode::InvalidClient
        ),
        "error was: {err}"
    );
    server.abort();
}

#[tokio::test]
async fn it_refreshes_tokens_and_surfaces_token_endpoint_errors() {
    let port = free_port();
    let base = format!("http://127.0.0.1:{port}");

    let mut app = App::new();
    app.map_post("/token", |form: Form<TokenForm>| async move {
        if form.grant_type != "refresh_token" {
            return volga::status!(400, { "error": "unsupported_grant_type" });
        }
        match form.refresh_token.as_deref() {
            Some("r1") => volga::ok!({
                "access_token": "fresh",
                "token_type": "Bearer",
                "expires_in": 3600
            }),
            _ => volga::status!(400, { "error": "invalid_grant" }),
        }
    });
    let server = serve(port, app).await;

    let metadata = server_metadata(&base);
    let client = plaintext_client("my-client");

    let tokens = client.refresh(&metadata, "r1").await.unwrap();
    assert_eq!(tokens.access_token, "fresh");

    let err = client.refresh(&metadata, "revoked").await.unwrap_err();
    assert!(
        matches!(
            &err,
            ClientError::Protocol(oauth) if oauth.error == OAuthErrorCode::InvalidGrant
        ),
        "error was: {err}"
    );
    server.abort();
}

#[tokio::test]
async fn it_serves_and_refreshes_tokens_through_the_store() {
    let port = free_port();
    let base = format!("http://127.0.0.1:{port}");

    let mut app = App::new();
    app.map_post("/token", |form: Form<TokenForm>| async move {
        match form.refresh_token.as_deref() {
            // rotated refresh token
            Some("r1") => volga::ok!({
                "access_token": "fresh-1",
                "token_type": "Bearer",
                "expires_in": 3600,
                "refresh_token": "r2"
            }),
            // no rotation — the old refresh token stays valid
            Some("r-keep") => volga::ok!({
                "access_token": "fresh-2",
                "token_type": "Bearer",
                "expires_in": 3600
            }),
            _ => volga::status!(400, { "error": "invalid_grant" }),
        }
    });
    let server = serve(port, app).await;

    let metadata = server_metadata(&base);
    let store = Arc::new(InMemoryTokenStore::new());
    let client = plaintext_client("my-client").with_token_store(store.clone());

    let expired = SystemTime::now() - Duration::from_secs(3600);
    let valid = SystemTime::now() + Duration::from_secs(3600);

    // nothing stored — interactive authorization required
    assert!(client.token("nobody", &metadata).await.unwrap().is_none());

    // still valid — served from the store without touching the server
    client.store_tokens("alice", &stored("a1", Some("r1"), valid));
    let tokens = client.token("alice", &metadata).await.unwrap().unwrap();
    assert_eq!(tokens.access_token, "a1");

    // expired — refreshed transparently, rotated refresh token persisted
    client.store_tokens("bob", &stored("b1", Some("r1"), expired));
    let tokens = client.token("bob", &metadata).await.unwrap().unwrap();
    assert_eq!(tokens.access_token, "fresh-1");
    assert_eq!(
        store.get("bob").unwrap().refresh_token.as_deref(),
        Some("r2")
    );

    // no rotation in the response — the old refresh token is carried over
    client.store_tokens("carol", &stored("c1", Some("r-keep"), expired));
    let tokens = client.token("carol", &metadata).await.unwrap().unwrap();
    assert_eq!(tokens.access_token, "fresh-2");
    assert_eq!(
        store.get("carol").unwrap().refresh_token.as_deref(),
        Some("r-keep")
    );

    // rejected refresh token — dead entry removed, authorization required
    client.store_tokens("dave", &stored("d1", Some("revoked"), expired));
    assert!(client.token("dave", &metadata).await.unwrap().is_none());
    assert!(store.get("dave").is_none());

    // expired without a refresh token — same outcome, no server round-trip
    client.store_tokens("erin", &stored("e1", None, expired));
    assert!(client.token("erin", &metadata).await.unwrap().is_none());
    assert!(store.get("erin").is_none());
    server.abort();
}

#[tokio::test]
async fn it_requires_a_token_endpoint_in_metadata() {
    let mut metadata = server_metadata("https://auth.example.com");
    metadata.token_endpoint = None;

    let client = OAuthClient::new("my-client");
    let auth = client.authorization_request(&metadata).build().unwrap();
    let err = client
        .exchange_code(&metadata, "c0de", &auth)
        .await
        .unwrap_err();
    assert!(
        matches!(&err, ClientError::Validation(reason) if reason.contains("token_endpoint")),
        "error was: {err}"
    );
}
