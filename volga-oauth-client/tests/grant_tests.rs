//! End-to-end tests for the grants that authenticate the client itself:
//! `client_credentials`, the RFC 7523 JWT bearer grant and RFC 8693 token
//! exchange, against a real volga application playing the token endpoint.
//! Same HTTP/1-only gating as the discovery suite (see `discovery_tests.rs`).

#![cfg(feature = "http1")]

mod common;

use std::{
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    time::{Duration, SystemTime},
};

use common::{free_port, serve};
#[cfg(feature = "private-key-jwt")]
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode};
use serde::Deserialize;
use volga::{
    App, Form,
    headers::{Authorization, Header},
};
use volga_oauth_client::{
    AuthorizationServerMetadata, ClientConfig, ClientError, InMemoryTokenStore, OAuthClient,
    OAuthErrorCode, TokenSet, TokenStore, grant, token_type,
};
#[cfg(feature = "private-key-jwt")]
use volga_oauth_client::{JwsAlgorithm, PrivateKeyJwt, client_auth};

/// A throwaway P-256 key pair; the client signs with the private half,
/// the test token endpoint verifies with the public one.
#[cfg(feature = "private-key-jwt")]
const CLIENT_KEY_PEM: &[u8] = b"-----BEGIN PRIVATE KEY-----
MIGHAgEAMBMGByqGSM49AgEGCCqGSM49AwEHBG0wawIBAQQgIeRig+AlqV2rBdgt
BzEQ28UAk8/d5l2+4PDfsspynmShRANCAATP07xL4i2PpomWJmZSZZMbQqj4Ybbd
aLozept2OHnD6J7pNTHm12NdaEJ4knzrCkp6pho2EFIQh5cKnqHm+hQw
-----END PRIVATE KEY-----";

#[cfg(feature = "private-key-jwt")]
const CLIENT_PUBLIC_KEY_PEM: &[u8] = b"-----BEGIN PUBLIC KEY-----
MFkwEwYHKoZIzj0CAQYIKoZIzj0DAQcDQgAEz9O8S+Itj6aJliZmUmWTG0Ko+GG2
3Wi6M3qbdjh5w+ie6TUx5tdjXWhCeJJ86wpKeqYaNhBSEIeXCp6h5voUMA==
-----END PUBLIC KEY-----";

/// The fields our fake token endpoint cares about.
#[derive(Deserialize)]
struct TokenForm {
    grant_type: String,
    scope: Option<String>,
    resource: Option<String>,
    #[cfg_attr(not(feature = "private-key-jwt"), allow(dead_code))]
    client_id: Option<String>,
    client_secret: Option<String>,
    client_assertion: Option<String>,
    #[cfg_attr(not(feature = "private-key-jwt"), allow(dead_code))]
    client_assertion_type: Option<String>,
    assertion: Option<String>,
    subject_token: Option<String>,
    subject_token_type: Option<String>,
    requested_token_type: Option<String>,
    audience: Option<String>,
}

/// The claims of a `private_key_jwt` client assertion.
#[cfg(feature = "private-key-jwt")]
#[derive(Deserialize)]
struct AssertionClaims {
    sub: String,
    jti: String,
}

/// An OAuth client accepting the plaintext test server.
fn plaintext_client(client_id: &str) -> OAuthClient {
    OAuthClient::new(client_id).with_config(ClientConfig::new().require_https(false))
}

/// Metadata advertising every grant this suite exercises.
fn server_metadata(base: &str) -> AuthorizationServerMetadata {
    let mut metadata = AuthorizationServerMetadata::new(base);
    metadata.token_endpoint = Some(format!("{base}/token"));
    metadata.grant_types_supported = vec![
        grant::CLIENT_CREDENTIALS.into(),
        grant::JWT_BEARER.into(),
        grant::TOKEN_EXCHANGE.into(),
    ];
    metadata
}

#[cfg(feature = "private-key-jwt")]
fn key() -> PrivateKeyJwt {
    PrivateKeyJwt::from_pem(CLIENT_KEY_PEM, JwsAlgorithm::ES256).unwrap()
}

#[cfg(feature = "private-key-jwt")]
/// Verifies a client assertion the way an authorization server would:
/// signature, `iss`/`sub` = the client, `aud` = the issuer, unexpired.
/// Returns the `jti`, so a caller can check it is fresh.
fn verify_assertion(assertion: &str, client_id: &str, issuer: &str) -> String {
    let mut validation = Validation::new(Algorithm::ES256);
    validation.set_issuer(&[client_id]);
    validation.set_audience(&[issuer]);

    let key = DecodingKey::from_ec_pem(CLIENT_PUBLIC_KEY_PEM).unwrap();
    let claims = decode::<AssertionClaims>(assertion, &key, &validation)
        .expect("the client assertion must verify")
        .claims;

    assert_eq!(claims.sub, client_id);
    claims.jti
}

#[tokio::test]
async fn it_requests_a_token_with_client_credentials() {
    let port = free_port();
    let base = format!("http://127.0.0.1:{port}");

    // client_secret_basic: base64("my-service:s3cret")
    let mut app = App::new();
    app.map_post(
        "/token",
        |authorization: Header<Authorization>, form: Form<TokenForm>| async move {
            if form.grant_type != grant::CLIENT_CREDENTIALS {
                return volga::status!(400, { "error": "unsupported_grant_type" });
            }
            match authorization.as_str() {
                Ok("Basic bXktc2VydmljZTpzM2NyZXQ=") => volga::ok!({
                    "access_token": "at",
                    "token_type": "Bearer",
                    "expires_in": 3600,
                    // echoed back so the client side can assert both went through
                    "scope": format!(
                        "{} {}",
                        form.scope.clone().unwrap_or_default(),
                        form.resource.clone().unwrap_or_default()
                    )
                }),
                _ => volga::status!(401, { "error": "invalid_client" }),
            }
        },
    );
    let server = serve(port, app).await;

    let metadata = server_metadata(&base);
    let tokens = plaintext_client("my-service")
        .with_secret("s3cret")
        .client_credentials(&metadata)
        .with_scopes(["inventory:read", "inventory:write"])
        .with_resource("https://api.example.com")
        .send()
        .await
        .unwrap();

    assert_eq!(tokens.access_token, "at");
    assert_eq!(
        tokens.scope.as_deref(),
        Some("inventory:read inventory:write https://api.example.com")
    );
    // the client credentials grant issues no refresh token (RFC 6749 Section 4.4.3)
    assert_eq!(tokens.refresh_token, None);
    assert!(!tokens.is_expired());
    server.abort();
}

#[tokio::test]
async fn it_serves_a_stored_service_token_and_re_requests_it_when_stale() {
    let port = free_port();
    let base = format!("http://127.0.0.1:{port}");

    // every request gets a distinct token, so the client side can tell a
    // cache hit from a fresh grant
    let issued = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&issued);
    let mut app = App::new();
    app.map_post("/token", move |form: Form<TokenForm>| {
        let counter = Arc::clone(&counter);
        async move {
            if form.grant_type != grant::CLIENT_CREDENTIALS {
                return volga::status!(400, { "error": "unsupported_grant_type" });
            }
            let nth = counter.fetch_add(1, Ordering::SeqCst) + 1;
            volga::ok!({
                "access_token": format!("at-{nth}"),
                "token_type": "Bearer",
                // short enough that the second token is already stale by
                // the leeway, and no refresh token - as RFC 6749
                // Section 4.4.3 prescribes for this grant
                "expires_in": if nth == 1 { 3600 } else { 5 }
            })
        }
    });
    let server = serve(port, app).await;

    let metadata = server_metadata(&base);
    let store = Arc::new(InMemoryTokenStore::new());
    let client = plaintext_client("my-service")
        .with_secret("s3cret")
        .with_token_store(store.clone());

    let service_token = || {
        client
            .client_credentials(&metadata)
            .with_scopes(["inventory:read"])
            .token("inventory")
    };

    // nothing stored yet - the grant runs
    assert_eq!(service_token().await.unwrap().access_token, "at-1");
    // ...and the next call is served from the store
    assert_eq!(service_token().await.unwrap().access_token, "at-1");
    assert_eq!(issued.load(Ordering::SeqCst), 1);

    // a token that expires within the leeway is replaced, even though the
    // grant issues nothing to refresh it with
    store.put(
        "inventory",
        &TokenSet {
            access_token: "stale".into(),
            token_type: "Bearer".into(),
            refresh_token: None,
            scope: None,
            id_token: None,
            expires_at: Some(SystemTime::now() + Duration::from_secs(5)),
            dpop_jkt: None,
        },
    );
    assert_eq!(service_token().await.unwrap().access_token, "at-2");
    assert_eq!(issued.load(Ordering::SeqCst), 2);

    // the freshly stored one is itself stale, so it is not served back
    assert_eq!(service_token().await.unwrap().access_token, "at-3");

    // `OAuthClient::token` cannot serve this grant: with no refresh token
    // it can only report that authorization is needed
    assert!(
        client
            .token("inventory", &metadata)
            .await
            .unwrap()
            .is_none()
    );

    // a stored token whose lifetime the server never stated is no evidence
    // of freshness, so it is re-requested rather than served forever
    store.put(
        "inventory",
        &TokenSet {
            access_token: "no-known-lifetime".into(),
            token_type: "Bearer".into(),
            refresh_token: None,
            scope: None,
            id_token: None,
            expires_at: None,
            dpop_jkt: None,
        },
    );
    let issued_before = issued.load(Ordering::SeqCst);
    let tokens = service_token().await.unwrap();
    assert_ne!(tokens.access_token, "no-known-lifetime");
    assert_eq!(issued.load(Ordering::SeqCst), issued_before + 1);

    server.abort();
}

#[cfg(feature = "private-key-jwt")]
#[tokio::test]
async fn it_requests_a_token_with_a_private_key_jwt_assertion() {
    let port = free_port();
    let base = format!("http://127.0.0.1:{port}");
    let issuer = base.clone();

    let mut app = App::new();
    app.map_post("/token", move |form: Form<TokenForm>| {
        let issuer = issuer.clone();
        async move {
            if form.grant_type != grant::CLIENT_CREDENTIALS
                || form.client_assertion_type.as_deref()
                    != Some(client_auth::ASSERTION_TYPE_JWT_BEARER)
                || form.client_id.as_deref() != Some("my-service")
            {
                return volga::status!(400, { "error": "invalid_request" });
            }
            // no shared secret is involved in this profile
            if form.client_secret.is_some() {
                return volga::status!(400, { "error": "invalid_request" });
            }
            let Some(assertion) = form.client_assertion.as_deref() else {
                return volga::status!(401, { "error": "invalid_client" });
            };
            let jti = verify_assertion(assertion, "my-service", &issuer);
            volga::ok!({ "access_token": jti, "token_type": "Bearer", "expires_in": 3600 })
        }
    });
    let server = serve(port, app).await;

    let metadata = server_metadata(&base);
    let client = plaintext_client("my-service").with_private_key_jwt(key());

    let first = client.client_credentials(&metadata).send().await.unwrap();
    let second = client.client_credentials(&metadata).send().await.unwrap();

    // the endpoint echoes the verified `jti` back: every request carries a
    // freshly signed, non-replayable assertion
    assert!(!first.access_token.is_empty());
    assert_ne!(first.access_token, second.access_token);
    server.abort();
}

#[tokio::test]
async fn it_presents_a_workload_jwt_as_an_authorization_grant() {
    let port = free_port();
    let base = format!("http://127.0.0.1:{port}");

    let mut app = App::new();
    app.map_post("/token", |form: Form<TokenForm>| async move {
        if form.grant_type != grant::JWT_BEARER {
            return volga::status!(400, { "error": "unsupported_grant_type" });
        }
        // the assertion is the credential - the client authenticates with
        // nothing else
        if form.client_secret.is_some() || form.client_assertion.is_some() {
            return volga::status!(400, { "error": "invalid_request" });
        }
        match form.assertion.as_deref() {
            Some("the.workload.jwt") => volga::ok!({
                "access_token": "at",
                "token_type": "Bearer",
                "scope": form.scope.clone().unwrap_or_default()
            }),
            _ => volga::status!(400, { "error": "invalid_grant" }),
        }
    });
    let server = serve(port, app).await;

    let metadata = server_metadata(&base);
    let client = plaintext_client("my-workload");

    let tokens = client
        .jwt_bearer(&metadata, "the.workload.jwt")
        .with_scopes(["inventory:read"])
        .send()
        .await
        .unwrap();
    assert_eq!(tokens.access_token, "at");
    assert_eq!(tokens.scope.as_deref(), Some("inventory:read"));

    // a rejected assertion is final: it surfaces as invalid_grant, and the
    // caller is expected not to retry it or switch grant types
    let err = client
        .jwt_bearer(&metadata, "stale.workload.jwt")
        .send()
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        ClientError::Protocol(err) if err.error == OAuthErrorCode::InvalidGrant
    ));
    server.abort();
}

#[tokio::test]
async fn it_exchanges_an_id_token_then_presents_the_assertion() {
    // the enterprise-managed profile end to end: exchange the user's ID
    // token at the identity provider for a cross-domain assertion, then
    // present that assertion to the resource's authorization server
    let idp_port = free_port();
    let idp_base = format!("http://127.0.0.1:{idp_port}");
    let as_port = free_port();
    let as_base = format!("http://127.0.0.1:{as_port}");

    let mut idp = App::new();
    idp.map_post("/token", |form: Form<TokenForm>| async move {
        if form.grant_type != grant::TOKEN_EXCHANGE
            || form.subject_token.as_deref() != Some("the.id.token")
            || form.subject_token_type.as_deref() != Some(token_type::ID_TOKEN)
            || form.requested_token_type.as_deref() != Some(token_type::ID_JAG)
            || form.audience.as_deref() != Some("https://api.example.com")
        {
            return volga::status!(400, { "error": "invalid_request" });
        }
        volga::ok!({
            "access_token": "the.id.jag",
            "issued_token_type": token_type::ID_JAG,
            "token_type": "N_A",
            "expires_in": 300
        })
    });
    let idp_server = serve(idp_port, idp).await;

    let mut authorization_server = App::new();
    authorization_server.map_post("/token", |form: Form<TokenForm>| async move {
        if form.grant_type != grant::JWT_BEARER || form.assertion.as_deref() != Some("the.id.jag") {
            return volga::status!(400, { "error": "invalid_grant" });
        }
        volga::ok!({ "access_token": "at", "token_type": "Bearer", "expires_in": 3600 })
    });
    let as_server = serve(as_port, authorization_server).await;

    let client = plaintext_client("my-app").with_secret("s3cret");
    let idp_metadata = server_metadata(&idp_base);
    let as_metadata = server_metadata(&as_base);

    let exchanged = client
        .exchange_token(&idp_metadata, "the.id.token", token_type::ID_TOKEN)
        .with_requested_token_type(token_type::ID_JAG)
        .with_audience("https://api.example.com")
        .send()
        .await
        .unwrap();

    assert_eq!(exchanged.token, "the.id.jag");
    assert_eq!(exchanged.issued_token_type, token_type::ID_JAG);
    // an ID-JAG is not a bearer token - it is only good as a grant
    assert!(!exchanged.is_bearer());
    assert!(!exchanged.is_expired());

    let tokens = client
        .jwt_bearer(&as_metadata, &exchanged.token)
        .send()
        .await
        .unwrap();
    assert_eq!(tokens.access_token, "at");

    idp_server.abort();
    as_server.abort();
}

#[tokio::test]
async fn it_surfaces_token_endpoint_errors() {
    let port = free_port();
    let base = format!("http://127.0.0.1:{port}");

    let mut app = App::new();
    app.map_post("/token", |form: Form<TokenForm>| async move {
        match form.scope.as_deref() {
            Some("forbidden") => volga::status!(400, {
                "error": "invalid_scope",
                "error_description": "scope 'forbidden' is not granted to this client"
            }),
            Some("wrong-grant") => volga::status!(400, { "error": "unauthorized_client" }),
            _ => volga::ok!({ "access_token": "at", "token_type": "Bearer" }),
        }
    });
    let server = serve(port, app).await;

    let metadata = server_metadata(&base);
    let client = plaintext_client("my-service").with_secret("s3cret");

    let err = client
        .client_credentials(&metadata)
        .with_scopes(["forbidden"])
        .send()
        .await
        .unwrap_err();
    let ClientError::Protocol(err) = err else {
        panic!("expected a protocol error, got {err:?}");
    };
    assert_eq!(err.error, OAuthErrorCode::InvalidScope);
    assert!(err.error_description.is_some());

    let err = client
        .client_credentials(&metadata)
        .with_scopes(["wrong-grant"])
        .send()
        .await
        .unwrap_err();
    assert!(matches!(
        err,
        ClientError::Protocol(err) if err.error == OAuthErrorCode::UnauthorizedClient
    ));
    server.abort();
}

#[tokio::test]
async fn it_refuses_an_unadvertised_grant_before_any_request() {
    // nothing is listening on this port: reaching the network at all would
    // surface as a transport error instead
    let base = format!("http://127.0.0.1:{}", free_port());
    let mut metadata = server_metadata(&base);
    metadata.grant_types_supported = vec!["authorization_code".into()];

    let err = plaintext_client("my-service")
        .with_secret("s3cret")
        .client_credentials(&metadata)
        .send()
        .await
        .unwrap_err();

    assert!(matches!(
        err,
        ClientError::Validation(reason) if reason.contains("client_credentials")
    ));
}
