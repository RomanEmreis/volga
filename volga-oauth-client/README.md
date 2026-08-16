# volga-oauth-client

OAuth 2.1 / OpenID Connect client for the [Volga](https://crates.io/crates/volga) Web Framework.

Built on the shared protocol types from `volga-oauth-core` and independent of the `volga` server crate - usable from any Tokio application.

Provides:

* Discovery client fetching Authorization Server Metadata ([RFC 8414](https://www.rfc-editor.org/rfc/rfc8414)) and Protected Resource Metadata ([RFC 9728](https://www.rfc-editor.org/rfc/rfc9728))
* Authorization Code flow with mandatory PKCE (S256, [RFC 7636](https://www.rfc-editor.org/rfc/rfc7636)), refresh tokens and resource indicators ([RFC 8707](https://www.rfc-editor.org/rfc/rfc8707))
* The grants that authenticate the client itself: client credentials ([RFC 6749](https://www.rfc-editor.org/rfc/rfc6749) Section 4.4), the JWT bearer grant ([RFC 7523](https://www.rfc-editor.org/rfc/rfc7523) Section 2.1) and token exchange ([RFC 8693](https://www.rfc-editor.org/rfc/rfc8693))
* Client authentication with `client_secret_basic`, `client_secret_post` or `private_key_jwt` (RFC 7523 Section 2.2)
* Token persistence and transparent refresh through the `TokenStore` abstraction
* Dynamic Client Registration ([RFC 7591](https://www.rfc-editor.org/rfc/rfc7591)) - the RFC 7592 management protocol is not implemented, but the `registration_access_token` / `registration_client_uri` pair is surfaced for applications that need it

## Example

```rust,no_run
use std::sync::Arc;
use volga_oauth_client::{ClientError, DiscoveryClient, InMemoryTokenStore, OAuthClient};

async fn authorize() -> Result<(), ClientError> {
    let metadata = DiscoveryClient::new()
        .fetch_server_metadata("https://auth.example.com")
        .await?;

    let client = OAuthClient::new("my-client")
        .with_redirect_uri("https://app.example.com/callback")
        .with_token_store(Arc::new(InMemoryTokenStore::new()));

    let auth = client
        .authorization_request(&metadata)
        .with_scopes(["read"])
        .with_resource("https://api.example.com")
        .build()?;

    // send the user to `auth.url`; then, in the redirect callback:
    let (code, state) = ("code", "state");
    assert!(auth.matches_state(state));
    let tokens = client.exchange_code(&metadata, code, &auth).await?;
    client.store_tokens("alice", &tokens);

    // later - served from the store, transparently refreshed when stale:
    let tokens = client.token("alice", &metadata).await?;
    Ok(())
}
```

### Machine-to-machine

No user is involved, so the client's own credentials are the grant:

```rust,no_run
use volga_oauth_client::{
    ClientError, DiscoveryClient, OAuthClient, PrivateKeyJwt, SigningAlgorithm,
};

async fn service_token(signing_key_pem: &[u8]) -> Result<(), ClientError> {
    let metadata = DiscoveryClient::new()
        .fetch_server_metadata("https://auth.example.com")
        .await?;

    // authenticating with a key of its own instead of a shared secret
    let client = OAuthClient::new("my-service")
        .with_private_key_jwt(PrivateKeyJwt::from_pem(
            signing_key_pem,
            SigningAlgorithm::RS256,
        )?);

    let tokens = client
        .client_credentials(&metadata)
        .with_scopes(["inventory:read"])
        .with_resource("https://api.example.com")
        .send()
        .await?;
    Ok(())
}
```

`jwt_bearer` presents a JWT the caller already holds (a workload identity token, say) as the grant, and `exchange_token` implements RFC 8693 - trading one token for another, possibly of a different type.

## Feature flags

| Flag | What it enables |
|---|---|
| `http1` (default) | HTTP/1.1 via hyper |
| `http2` | HTTP/2 via hyper; negotiated through TLS ALPN when combined with `http1`, used exclusively (prior knowledge over plaintext) without it |

At least one of the two must be enabled.
