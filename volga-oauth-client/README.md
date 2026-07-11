# volga-oauth-client

OAuth 2.1 / OpenID Connect client for the [Volga](https://crates.io/crates/volga) Web Framework.

Built on the shared protocol types from `volga-oauth-core` and independent of the `volga` server crate - usable from any Tokio application.

Provides:

* Discovery client fetching Authorization Server Metadata ([RFC 8414](https://www.rfc-editor.org/rfc/rfc8414)) and Protected Resource Metadata ([RFC 9728](https://www.rfc-editor.org/rfc/rfc9728))
* Authorization Code flow with mandatory PKCE (S256, [RFC 7636](https://www.rfc-editor.org/rfc/rfc7636)), refresh tokens and resource indicators ([RFC 8707](https://www.rfc-editor.org/rfc/rfc8707))
* Token persistence and transparent refresh through the `TokenStore` abstraction

Roadmap:

* Dynamic Client Registration ([RFC 7591](https://www.rfc-editor.org/rfc/rfc7591))

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

    // later — served from the store, transparently refreshed when stale:
    let tokens = client.token("alice", &metadata).await?;
    Ok(())
}
```

## Feature flags

| Flag | What it enables |
|---|---|
| `http1` (default) | HTTP/1.1 via hyper |
| `http2` | HTTP/2 via hyper; negotiated through TLS ALPN when combined with `http1`, used exclusively (prior knowledge over plaintext) without it |

At least one of the two must be enabled.
