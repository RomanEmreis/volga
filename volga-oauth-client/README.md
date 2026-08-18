# volga-oauth-client

OAuth 2.1 / OpenID Connect client for the [Volga](https://crates.io/crates/volga) Web Framework.

Built on the shared protocol types from `volga-oauth-core` and independent of the `volga` server crate - usable from any Tokio application.

Provides:

* Discovery client fetching Authorization Server Metadata ([RFC 8414](https://www.rfc-editor.org/rfc/rfc8414)) and Protected Resource Metadata ([RFC 9728](https://www.rfc-editor.org/rfc/rfc9728))
* Authorization Code flow with mandatory PKCE (S256, [RFC 7636](https://www.rfc-editor.org/rfc/rfc7636)), refresh tokens and resource indicators ([RFC 8707](https://www.rfc-editor.org/rfc/rfc8707))
* The grants that authenticate the client itself: client credentials ([RFC 6749](https://www.rfc-editor.org/rfc/rfc6749) Section 4.4), the JWT bearer grant ([RFC 7523](https://www.rfc-editor.org/rfc/rfc7523) Section 2.1) and token exchange ([RFC 8693](https://www.rfc-editor.org/rfc/rfc8693))
* Client authentication with `client_secret_basic`, `client_secret_post` or `private_key_jwt` ([RFC 7523](https://www.rfc-editor.org/rfc/rfc7523) Section 2.2, feature `private-key-jwt`)
* DPoP sender-constrained tokens ([RFC 9449](https://www.rfc-editor.org/rfc/rfc9449), feature `dpop`) - proofs on every token request, the nonce round a server may demand, and the proofs a caller attaches to its own resource requests
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

No user is involved, so the client's own credentials are the grant. This example authenticates with a key of its own, which needs the `private-key-jwt` feature; with a client secret it is `with_secret("s3cret")` instead and no extra feature:

```rust,no_run
use volga_oauth_client::{
    ClientError, DiscoveryClient, JwsAlgorithm, OAuthClient, PrivateKeyJwt,
};

async fn service_token() -> Result<(), ClientError> {
    let metadata = DiscoveryClient::new()
        .fetch_server_metadata("https://auth.example.com")
        .await?;

    // authenticating with a key of its own instead of a shared secret
    // (`from_pem` takes the bytes directly when the key is not a file)
    let client = OAuthClient::new("my-service")
        .with_private_key_jwt(PrivateKeyJwt::from_pem_file(
            "/etc/secrets/client.pem",
            JwsAlgorithm::RS256,
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

### Sender-constrained tokens (DPoP)

A bearer token is a password: whoever holds it may use it. DPoP binds the token to a key the client holds, and every request carries a freshly signed proof of possession. Needs the `dpop` feature:

```rust,no_run
use http::{HeaderMap, Method};
use volga_oauth_client::{AuthorizationServerMetadata, ClientError, Dpop, OAuthClient};

async fn bound_token(metadata: &AuthorizationServerMetadata) -> Result<(), ClientError> {
    // one key per session; the client and the code making resource
    // requests share it, and cloning shares the nonce state with it
    let dpop = Dpop::generate()?;
    let client = OAuthClient::new("my-service")
        .with_secret("s3cret")
        .with_dpop(dpop.clone());

    // the token request carries a proof, including the nonce round the
    // server may demand - the token comes back bound to the key
    let tokens = client.client_credentials(metadata).send().await?;
    assert!(tokens.is_dpop());

    // resource requests are yours to make; this fills in
    // `Authorization: DPoP <token>` and the `DPoP` proof covering it
    let url = "https://api.example.com/orders";
    let mut headers = HeaderMap::new();
    dpop.authorize(&mut headers, &Method::GET, url, &tokens)?;

    // on a `use_dpop_nonce` refusal, adopt the nonce and repeat once:
    // `dpop.accept_nonce(url, response.headers())` returns whether the
    // nonce was new, which is what makes the retry worth making
    Ok(())
}
```

## Feature flags

| Flag | What it enables |
|---|---|
| `http1` (default) | HTTP/1.1 via hyper |
| `http2` | HTTP/2 via hyper; negotiated through TLS ALPN when combined with `http1`, used exclusively (prior knowledge over plaintext) without it |
| `private-key-jwt` | `private_key_jwt` client authentication ([RFC 7523](https://www.rfc-editor.org/rfc/rfc7523) Section 2.2) - `PrivateKeyJwt`, `ClientAuthMethod::PrivateKeyJwt`, `OAuthClient::with_private_key_jwt` and `from_registration_with_key` |
| `dpop` | DPoP sender-constrained tokens ([RFC 9449](https://www.rfc-editor.org/rfc/rfc9449)) - `Dpop`, `OAuthClient::with_dpop`, `TokenSet::is_dpop` |

At least one of `http1` / `http2` must be enabled.

`private-key-jwt` and `dpop` are off by default: they are the only parts of this crate that need a JWS signing backend (`jsonwebtoken` on `aws-lc-rs`). Everything else - every grant, `client_secret_basic`, `client_secret_post` and public clients - works without them. They are independent of each other: a `private_key_jwt` credential says who asked for a token, a DPoP proof says who holds it, and a request may carry both.
