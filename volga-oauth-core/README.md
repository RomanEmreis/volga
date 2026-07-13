# volga-oauth-core

Shared OAuth 2.1 / OpenID Connect foundation types for the [Volga](https://crates.io/crates/volga) Web Framework:

* Error models per [RFC 6749 §5.2](https://www.rfc-editor.org/rfc/rfc6749#section-5.2) and [RFC 6750 §3.1](https://www.rfc-editor.org/rfc/rfc6750#section-3.1)
* Authorization Server Metadata per [RFC 8414](https://www.rfc-editor.org/rfc/rfc8414)
* Protected Resource Metadata per [RFC 9728](https://www.rfc-editor.org/rfc/rfc9728)
* Dynamic Client Registration models per [RFC 7591](https://www.rfc-editor.org/rfc/rfc7591)
* `WWW-Authenticate` Bearer challenge builder and parser
* Resource URI canonicalization per [RFC 8707](https://www.rfc-editor.org/rfc/rfc8707) and well-known metadata URL derivation

This crate contains no HTTP I/O - it is the protocol-type layer shared by the `volga` server (metadata serving, bearer challenges) and the OAuth client crates. Most applications should depend on `volga` (with the `oauth` feature) or `volga-oauth-client` instead of this crate directly.
