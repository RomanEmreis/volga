//! Verifies the built-in OAuth metadata handlers (RFC 8414 §3, RFC 9728 §3,
//! OIDC Discovery 1.0 §4).

#![cfg(all(feature = "oauth", feature = "test"))]

use volga::{
    App,
    auth::oauth::{AuthorizationServerMetadata, ProtectedResourceMetadata},
    test::TestServer,
};

#[tokio::test]
async fn it_serves_resource_metadata() {
    let server = TestServer::builder()
        .setup(|app: &mut App| {
            let metadata = ProtectedResourceMetadata::new("https://api.example.com")
                .with_authorization_servers(["https://auth.example.com"])
                .with_scopes(["read", "write"]);
            app.use_oauth_resource_metadata(metadata);
        })
        .build()
        .await;

    let res = server
        .client()
        .get(server.url("/.well-known/oauth-protected-resource"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let content_type = res.headers().get("content-type").unwrap().to_str().unwrap();
    assert!(
        content_type.starts_with("application/json"),
        "content-type was: {content_type}"
    );
    let cache_control = res
        .headers()
        .get("cache-control")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(
        cache_control.contains("public"),
        "cache-control was: {cache_control}"
    );

    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["resource"], "https://api.example.com");
    assert_eq!(body["authorization_servers"][0], "https://auth.example.com");
    assert_eq!(
        body["scopes_supported"],
        serde_json::json!(["read", "write"])
    );
    server.shutdown().await;
}

#[tokio::test]
async fn it_serves_resource_metadata_for_path_resources() {
    let server = TestServer::builder()
        .setup(|app: &mut App| {
            // The well-known path is inserted between host and path (RFC 9728 §3.1)
            app.use_oauth_resource_metadata("https://api.example.com/api/v1");
        })
        .build()
        .await;

    let res = server
        .client()
        .get(server.url("/.well-known/oauth-protected-resource/api/v1"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["resource"], "https://api.example.com/api/v1");

    // The root well-known path is not mounted for a path-scoped resource
    let res = server
        .client()
        .get(server.url("/.well-known/oauth-protected-resource"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
    server.shutdown().await;
}

#[tokio::test]
async fn it_serves_server_metadata() {
    let server = TestServer::builder()
        .setup(|app: &mut App| {
            let metadata = AuthorizationServerMetadata::new("https://auth.example.com")
                .with_token_endpoint("https://auth.example.com/token");
            app.use_oauth_server_metadata(metadata);
        })
        .build()
        .await;

    let res = server
        .client()
        .get(server.url("/.well-known/oauth-authorization-server"))
        .send()
        .await
        .unwrap();

    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["issuer"], "https://auth.example.com");
    assert_eq!(body["token_endpoint"], "https://auth.example.com/token");
    assert_eq!(
        body["response_types_supported"],
        serde_json::json!(["code"])
    );
    assert_eq!(
        body["grant_types_supported"],
        serde_json::json!(["authorization_code"])
    );
    server.shutdown().await;
}

#[tokio::test]
async fn it_serves_openid_configuration() {
    let server = TestServer::builder()
        .setup(|app: &mut App| {
            let metadata = AuthorizationServerMetadata::new("https://auth.example.com")
                .with_jwks_uri("https://auth.example.com/jwks")
                .with_additional_field("subject_types_supported", serde_json::json!(["public"]));
            app.use_oauth_server_metadata(metadata.clone())
                .use_oidc_metadata(metadata);
        })
        .build()
        .await;

    // The same document is available at both discovery paths
    for path in [
        "/.well-known/openid-configuration",
        "/.well-known/oauth-authorization-server",
    ] {
        let res = server.client().get(server.url(path)).send().await.unwrap();
        assert_eq!(res.status(), 200, "path: {path}");
        let body: serde_json::Value = res.json().await.unwrap();
        assert_eq!(body["issuer"], "https://auth.example.com");
        assert_eq!(body["jwks_uri"], "https://auth.example.com/jwks");
        assert_eq!(
            body["subject_types_supported"],
            serde_json::json!(["public"])
        );
    }
    server.shutdown().await;
}

#[tokio::test]
async fn it_serves_openid_configuration_for_path_issuers() {
    let server = TestServer::builder()
        .setup(|app: &mut App| {
            // OIDC appends the well-known path after the issuer's path,
            // unlike the RFC 8414 insertion rule
            app.use_oidc_metadata("https://auth.example.com/tenant1");
        })
        .build()
        .await;

    let res = server
        .client()
        .get(server.url("/tenant1/.well-known/openid-configuration"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 200);
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["issuer"], "https://auth.example.com/tenant1");

    let res = server
        .client()
        .get(server.url("/.well-known/openid-configuration"))
        .send()
        .await
        .unwrap();
    assert_eq!(res.status(), 404);
    server.shutdown().await;
}

#[tokio::test]
async fn it_serves_minimal_metadata_from_identifier_strings() {
    let server = TestServer::builder()
        .setup(|app: &mut App| {
            app.use_oauth_resource_metadata("https://api.example.com")
                .use_oauth_server_metadata("https://auth.example.com");
        })
        .build()
        .await;

    let res = server
        .client()
        .get(server.url("/.well-known/oauth-protected-resource"))
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(
        body,
        serde_json::json!({ "resource": "https://api.example.com" })
    );

    let res = server
        .client()
        .get(server.url("/.well-known/oauth-authorization-server"))
        .send()
        .await
        .unwrap();
    let body: serde_json::Value = res.json().await.unwrap();
    assert_eq!(body["issuer"], "https://auth.example.com");
    server.shutdown().await;
}
