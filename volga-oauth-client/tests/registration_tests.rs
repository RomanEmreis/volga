//! End-to-end Dynamic Client Registration tests: a real volga application
//! playing the registration endpoint for [`RegistrationClient`]. Same
//! HTTP/1-only gating as the discovery suite (see `discovery_tests.rs`).

#![cfg(feature = "http1")]

mod common;

use common::{free_port, serve};
use volga::{App, HttpRequest, Json};
use volga_oauth_client::{
    AuthorizationServerMetadata, ClientError, ClientMetadata, OAuthClient, OAuthErrorCode,
    RegistrationClient,
};

fn server_metadata(base: &str) -> AuthorizationServerMetadata {
    let mut metadata = AuthorizationServerMetadata::new(base);
    metadata.registration_endpoint = Some(format!("{base}/register"));
    metadata
}

fn plaintext_client() -> RegistrationClient {
    RegistrationClient::with_config(volga_oauth_client::ClientConfig::new().require_https(false))
}

#[tokio::test]
async fn it_registers_a_client_and_builds_an_oauth_client_from_it() {
    let port = free_port();
    let base = format!("http://127.0.0.1:{port}");

    // the endpoint echoes the registered metadata back with credentials,
    // per RFC 7591 §3.2.1
    let mut app = App::new();
    app.map_post("/register", |request: Json<ClientMetadata>| async move {
        if request.redirect_uris.is_empty() {
            return volga::status!(400, { "error": "invalid_redirect_uri" });
        }
        let mut body = serde_json::to_value(&*request).unwrap();
        body["client_id"] = "generated-id".into();
        body["client_secret"] = "generated-secret".into();
        body["client_secret_expires_at"] = 0.into();
        body["token_endpoint_auth_method"] = "client_secret_post".into();
        volga::status!(201, body)
    });
    let server = serve(port, app).await;

    let registered = plaintext_client()
        .register(
            &server_metadata(&base),
            &ClientMetadata::new()
                .with_redirect_uris(["https://app.example.com/callback"])
                .with_client_name("My App")
                .with_scopes(["read"]),
        )
        .await
        .unwrap();

    assert_eq!(registered.client_id, "generated-id");
    assert_eq!(
        registered.client_secret.as_deref(),
        Some("generated-secret")
    );
    assert_eq!(registered.client_secret_expires_at, Some(0));
    // the server-registered metadata is what counts
    assert_eq!(registered.metadata.client_name.as_deref(), Some("My App"));
    assert_eq!(registered.metadata.grant_types, ["authorization_code"]);
    assert_eq!(
        registered.metadata.token_endpoint_auth_method.as_deref(),
        Some("client_secret_post")
    );

    // the issued credentials carry over into an OAuthClient
    let client = OAuthClient::from_registration(&registered);
    let debug = format!("{client:?}");
    assert!(debug.contains("generated-id"));
    assert!(debug.contains("Post"));
    assert!(debug.contains("https://app.example.com/callback"));
    server.abort();
}

#[tokio::test]
async fn it_sends_the_initial_access_token() {
    let port = free_port();
    let base = format!("http://127.0.0.1:{port}");

    let mut app = App::new();
    app.map_post("/register", |req: HttpRequest| async move {
        match req.headers().get(hyper::header::AUTHORIZATION) {
            Some(value) if value == "Bearer initial-token" => {
                volga::status!(201, { "client_id": "generated-id" })
            }
            _ => volga::status!(401, { "error": "invalid_token" }),
        }
    });
    let server = serve(port, app).await;
    let metadata = server_metadata(&base);
    let request = ClientMetadata::new().with_grant_types(["client_credentials"]);

    let registered = plaintext_client()
        .with_initial_access_token("initial-token")
        .register(&metadata, &request)
        .await
        .unwrap();
    assert_eq!(registered.client_id, "generated-id");
    assert_eq!(registered.client_secret, None);

    // without the token registration is rejected
    let err = plaintext_client()
        .register(&metadata, &request)
        .await
        .unwrap_err();
    assert!(
        matches!(
            &err,
            ClientError::Protocol(oauth) if oauth.error == OAuthErrorCode::InvalidToken
        ),
        "error was: {err}"
    );
    server.abort();
}

#[tokio::test]
async fn it_surfaces_rfc7591_error_codes() {
    let port = free_port();
    let base = format!("http://127.0.0.1:{port}");

    let mut app = App::new();
    app.map_post("/register", || async {
        volga::status!(400, {
            "error": "invalid_client_metadata",
            "error_description": "logo_uri is not a valid URL"
        })
    });
    let server = serve(port, app).await;

    let err = plaintext_client()
        .register(
            &server_metadata(&base),
            &ClientMetadata::new().with_redirect_uris(["https://app.example.com/callback"]),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(
            &err,
            ClientError::Protocol(oauth)
                if oauth.error == OAuthErrorCode::InvalidClientMetadata
        ),
        "error was: {err}"
    );
    server.abort();
}

#[tokio::test]
async fn it_validates_before_any_io() {
    // no server involved in either case
    let no_endpoint = AuthorizationServerMetadata::new("https://auth.example.com");
    let err = RegistrationClient::new()
        .register(&no_endpoint, &ClientMetadata::new())
        .await
        .unwrap_err();
    assert!(
        matches!(&err, ClientError::Validation(reason) if reason.contains("registration_endpoint")),
        "error was: {err}"
    );

    let err = RegistrationClient::new()
        .register(
            &server_metadata("https://auth.example.com"),
            &ClientMetadata::new(),
        )
        .await
        .unwrap_err();
    assert!(
        matches!(&err, ClientError::Validation(reason) if reason.contains("redirect_uris")),
        "error was: {err}"
    );
}
