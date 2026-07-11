# volga-oauth-client

OAuth 2.1 / OpenID Connect client for the [Volga](https://crates.io/crates/volga) Web Framework.

Built on the shared protocol types from `volga-oauth-core` and independent of the `volga` server crate - usable from any Tokio application.

Roadmap:

* Discovery client fetching Authorization Server Metadata ([RFC 8414](https://www.rfc-editor.org/rfc/rfc8414)) and Protected Resource Metadata ([RFC 9728](https://www.rfc-editor.org/rfc/rfc9728))
* Authorization Code flow with PKCE, refresh tokens and resource indicators ([RFC 8707](https://www.rfc-editor.org/rfc/rfc8707))
* Dynamic Client Registration ([RFC 7591](https://www.rfc-editor.org/rfc/rfc7591))

This crate currently provides the client configuration and error model; the flows above land incrementally.
