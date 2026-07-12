//! End-to-end discovery tests: a real volga application serving the
//! metadata documents (the `use_*` handlers) fetched by [`DiscoveryClient`].
//!
//! The dev-dependency volga server is built with `http1` (with `http2` it
//! serves HTTP/2 exclusively), and these plaintext tests have no ALPN, so
//! they require the client's `http1` feature. The HTTP/2 path was verified
//! end-to-end manually (`http2`-only client with prior knowledge against an
//! `http2` volga server passes this whole suite); in CI the `http2`-only
//! build is covered by unit tests and compilation.

#![cfg(feature = "http1")]

mod common;

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use common::{free_port, serve};
use volga::App;
use volga_oauth_client::{
    ClientConfig, ClientError, DiscoveryClient, MetadataCache, OAuthErrorCode,
};

/// A discovery client accepting the plaintext test server.
fn plaintext_client() -> DiscoveryClient {
    DiscoveryClient::with_config(ClientConfig::new().require_https(false))
}

#[tokio::test]
async fn it_fetches_server_metadata() {
    let port = free_port();
    let issuer = format!("http://127.0.0.1:{port}");

    let mut app = App::new().with_oauth_server_metadata(|m| {
        m.with_issuer(&issuer)
            .with_token_endpoint(format!("{issuer}/token"))
    });
    app.use_oauth_server_metadata();
    let server = serve(port, app).await;

    let metadata = plaintext_client()
        .fetch_server_metadata(&issuer)
        .await
        .unwrap();

    assert_eq!(metadata.issuer, issuer);
    assert_eq!(metadata.token_endpoint, Some(format!("{issuer}/token")));
    // the `new()` prefills survive the round-trip
    assert_eq!(metadata.response_types_supported, ["code"]);
    server.abort();
}

#[tokio::test]
async fn it_fetches_oidc_metadata_for_path_issuers() {
    let port = free_port();
    let issuer = format!("http://127.0.0.1:{port}/tenant1");

    let mut app = App::new().set_oauth_server_metadata(issuer.as_str());
    app.use_oidc_metadata();
    let server = serve(port, app).await;

    let metadata = plaintext_client()
        .fetch_oidc_metadata(&issuer)
        .await
        .unwrap();
    assert_eq!(metadata.issuer, issuer);
    server.abort();
}

#[tokio::test]
async fn it_fetches_resource_metadata_and_discovers_authorization_server() {
    let port = free_port();
    let base = format!("http://127.0.0.1:{port}");
    let resource = format!("{base}/api");

    // one app plays both roles: protected resource and authorization server
    let mut app = App::new()
        .with_oauth_resource_metadata(|m| {
            m.with_resource(&resource)
                .with_authorization_servers([&base])
                .with_scopes(["read"])
        })
        .set_oauth_server_metadata(base.as_str());
    app.use_oauth_resource_metadata()
        .use_oauth_server_metadata();
    let server = serve(port, app).await;

    let client = plaintext_client();
    let resource_metadata = client.fetch_resource_metadata(&resource).await.unwrap();
    assert_eq!(resource_metadata.resource, resource);
    assert_eq!(resource_metadata.scopes_supported, ["read"]);

    let server_metadata = client
        .discover_authorization_server(&resource_metadata)
        .await
        .unwrap();
    assert_eq!(server_metadata.issuer, base);
    server.abort();
}

#[tokio::test]
async fn it_falls_back_to_oidc_path_when_rfc8414_is_not_served() {
    let port = free_port();
    let base = format!("http://127.0.0.1:{port}");
    let resource = format!("{base}/api");

    // the authorization server publishes ONLY the OIDC discovery document
    let mut app = App::new()
        .with_oauth_resource_metadata(|m| {
            m.with_resource(&resource)
                .with_authorization_servers([&base])
        })
        .set_oauth_server_metadata(base.as_str());
    app.use_oauth_resource_metadata().use_oidc_metadata();
    let server = serve(port, app).await;

    let client = plaintext_client();
    let resource_metadata = client.fetch_resource_metadata(&resource).await.unwrap();
    let server_metadata = client
        .discover_authorization_server(&resource_metadata)
        .await
        .unwrap();
    assert_eq!(server_metadata.issuer, base);
    server.abort();
}

#[tokio::test]
async fn it_rejects_issuer_mismatch() {
    let port = free_port();
    let issuer = format!("http://127.0.0.1:{port}");

    // a document claiming a different issuer than the one it is fetched for
    let mut app = App::new();
    app.map_get("/.well-known/oauth-authorization-server", || async {
        volga::ok!({
            "issuer": "https://evil.example.com",
            "response_types_supported": ["code"]
        })
    });
    let server = serve(port, app).await;

    let err = plaintext_client()
        .fetch_server_metadata(&issuer)
        .await
        .unwrap_err();
    assert!(
        matches!(&err, ClientError::Validation(reason) if reason.contains("issuer mismatch")),
        "error was: {err}"
    );
    server.abort();
}

#[tokio::test]
async fn it_rejects_plain_http_by_default() {
    // no server involved — the URL is rejected before any I/O
    let err = DiscoveryClient::new()
        .fetch_server_metadata("http://auth.example.com")
        .await
        .unwrap_err();
    assert!(
        matches!(err, ClientError::InsecureUrl(_)),
        "error was: {err}"
    );
}

#[tokio::test]
async fn it_surfaces_oauth_error_bodies_and_bare_statuses() {
    let port = free_port();
    let issuer = format!("http://127.0.0.1:{port}");

    let mut app = App::new();
    app.map_get("/.well-known/oauth-authorization-server", || async {
        volga::status!(503, {
            "error": "temporarily_unavailable",
            "error_description": "maintenance"
        })
    });
    let server = serve(port, app).await;
    let client = plaintext_client();

    let err = client.fetch_server_metadata(&issuer).await.unwrap_err();
    assert!(
        matches!(
            &err,
            ClientError::Protocol(oauth)
                if oauth.error == OAuthErrorCode::TemporarilyUnavailable
        ),
        "error was: {err}"
    );

    // a missing document is a bare HTTP error, not a protocol error
    let err = client.fetch_oidc_metadata(&issuer).await.unwrap_err();
    assert!(
        matches!(err, ClientError::Http(status) if status == 404),
        "error was: {err}"
    );
    server.abort();
}

#[tokio::test]
async fn it_follows_redirects_within_the_configured_limit() {
    let port = free_port();
    let base = format!("http://127.0.0.1:{port}");
    let resource = base.clone();

    let mut app = App::new().set_oauth_resource_metadata(resource.as_str());
    app.use_oauth_resource_metadata();
    app.map_get("/moved", || async {
        volga::status!(302; [("Location", "/.well-known/oauth-protected-resource")])
    });
    let server = serve(port, app).await;

    let metadata = plaintext_client()
        .fetch_resource_metadata_from_url(&format!("{base}/moved"), Some(&resource))
        .await
        .unwrap();
    assert_eq!(metadata.resource, resource);

    // with redirects disabled the same URL fails
    let strict = DiscoveryClient::with_config(
        ClientConfig::new()
            .require_https(false)
            .with_max_redirects(0),
    );
    let err = strict
        .fetch_resource_metadata_from_url(&format!("{base}/moved"), None)
        .await
        .unwrap_err();
    assert!(
        matches!(&err, ClientError::Transport(source) if source.to_string().contains("too many redirects")),
        "error was: {err}"
    );
    server.abort();
}

#[derive(Debug, Default)]
struct ToyCache {
    documents: Mutex<HashMap<String, serde_json::Value>>,
}

impl MetadataCache for ToyCache {
    fn get(&self, url: &str) -> Option<serde_json::Value> {
        self.documents.lock().unwrap().get(url).cloned()
    }

    fn put(&self, url: &str, document: &serde_json::Value) {
        self.documents
            .lock()
            .unwrap()
            .insert(url.to_owned(), document.clone());
    }
}

#[tokio::test]
async fn it_caches_only_validated_documents() {
    let port = free_port();
    let issuer = format!("http://127.0.0.1:{port}");

    let mut app = App::new();
    // a document claiming a different issuer than the one it is fetched for
    app.map_get("/.well-known/oauth-authorization-server", || async {
        volga::ok!({
            "issuer": "https://evil.example.com",
            "response_types_supported": ["code"]
        })
    });
    // a 200 response that is not a metadata document at all
    app.map_get("/.well-known/openid-configuration", || async {
        volga::ok!({ "hello": "world" })
    });
    let server = serve(port, app).await;

    let cache = Arc::new(ToyCache::default());
    let client = DiscoveryClient::with_config(ClientConfig::new().require_https(false))
        .with_cache(cache.clone());

    // neither the lying nor the malformed response makes it into the cache
    assert!(client.fetch_server_metadata(&issuer).await.is_err());
    assert!(client.fetch_oidc_metadata(&issuer).await.is_err());
    assert!(cache.documents.lock().unwrap().is_empty());
    server.abort();
}

#[tokio::test]
async fn it_serves_documents_from_the_cache() {
    let port = free_port();
    let issuer = format!("http://127.0.0.1:{port}");

    let mut app = App::new().set_oauth_server_metadata(issuer.as_str());
    app.use_oauth_server_metadata();
    let server = serve(port, app).await;

    let cache = Arc::new(ToyCache::default());
    let client = DiscoveryClient::with_config(ClientConfig::new().require_https(false))
        .with_cache(cache.clone());

    let first = client.fetch_server_metadata(&issuer).await.unwrap();
    assert_eq!(cache.documents.lock().unwrap().len(), 1);

    // the server is gone — the second fetch can only succeed from the cache
    server.abort();
    let second = client.fetch_server_metadata(&issuer).await.unwrap();
    assert_eq!(first, second);

    // an uncached client fails against the stopped server
    let err = plaintext_client().fetch_server_metadata(&issuer).await;
    assert!(err.is_err());
}
