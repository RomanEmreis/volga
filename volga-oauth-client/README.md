# volga-oauth-client

OAuth 2.1 / OpenID Connect client for the [Volga](https://crates.io/crates/volga) Web Framework.

Built on the shared protocol types from `volga-oauth-core` and independent of the `volga` server crate - usable from any Tokio application.

Roadmap:

* Discovery client fetching Authorization Server Metadata ([RFC 8414](https://www.rfc-editor.org/rfc/rfc8414)) and Protected Resource Metadata ([RFC 9728](https://www.rfc-editor.org/rfc/rfc9728))
* Authorization Code flow with PKCE, refresh tokens and resource indicators ([RFC 8707](https://www.rfc-editor.org/rfc/rfc8707))
* Dynamic Client Registration ([RFC 7591](https://www.rfc-editor.org/rfc/rfc7591))

This crate currently provides the discovery client, configuration and error model; the flows above land incrementally.

## Feature flags

| Flag | What it enables |
|---|---|
| `http1` (default) | HTTP/1.1 via hyper |
| `http2` | HTTP/2 via hyper; negotiated through TLS ALPN when combined with `http1`, used exclusively (prior knowledge over plaintext) without it |

At least one of the two must be enabled.
