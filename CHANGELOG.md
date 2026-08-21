# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/).

# 0.9.8

## Added
* The grants that authenticate the **client itself** in `volga-oauth-client` - the machine-to-machine profiles, where no user is involved and the client is the subject (#212). Each is a builder taking `with_scopes` / `with_resource` (RFC 8707) / `with_param`, sent with `send()`:
  * `OAuthClient::client_credentials` - RFC 6749 Section 4.4. There is no authorization request to carry scopes here, so they go on the token request itself.
  * `OAuthClient::jwt_bearer` - RFC 7523 Section 2.1. The assertion is supplied by the caller rather than minted here: it is what some other authority already issued, a workload identity token or an identity assertion from a prior exchange.
  * `OAuthClient::exchange_token` - RFC 8693 token exchange, parameterised on `subject_token`, `subject_token_type`, `with_requested_token_type`, `with_audience`, `with_resource` and `with_actor_token`. It may hand back something other than a bearer access token, so it answers with `ExchangedToken` (carrying `issued_token_type` and `is_bearer()`) rather than `TokenSet`.
  * All three refuse a grant before reaching the network unless both sides allow it: the server has to list it in its `grant_types_supported`, and a client built by `OAuthClient::from_registration` has to have had it approved in the registration's `grant_types`. The registration check applies to the Authorization Code flow too - `authorization_request().build()` and `exchange_code` refuse it rather than redirect a user into a flow the client may not complete. An omitted `grant_types` is `authorization_code` alone, per RFC 7591 Section 2 and the same reading the registration request validator already used; only a client that never went through a registration is unconstrained. `refresh_token` is never refused: RFC 6749 Section 6 makes it the continuation of a grant already held, and servers routinely issue refresh tokens without naming it in a registration.
* `ClientCredentialsRequest::token(key)` - the store-backed counterpart of `OAuthClient::token` for a service token. It cannot be served by that method: RFC 6749 Section 4.4.3 issues no refresh token for this grant, so there is nothing to renew a stored token *with* - re-running the grant is the renewal, and the request carries the scopes and resource indicators to run it the same way again. A stored token whose lifetime the server never stated (`expires_in` is only RECOMMENDED by RFC 6749 Section 5.1) is re-requested rather than served: an unknown lifetime is no evidence of freshness, and holding one would keep handing out a credential long after it died.
* `private_key_jwt` client authentication (RFC 7523 Section 2.2, `volga-oauth-client` feature `private-key-jwt`) - a client assertion signed with the client's own key, so no shared secret ever leaves it. The feature is off by default because signing is the only part of the crate that needs a JWS backend (`jsonwebtoken` on `aws-lc-rs`); every grant, the secret-based methods and public clients work without it. `PrivateKeyJwt` loads the key (`from_pem`, `from_pem_file`, `from_der`) and carries the claims policy (`with_key_id`, `with_lifetime`, `with_audiences`); attach it with `OAuthClient::with_private_key_jwt` or `OAuthClient::from_registration_with_key` and it applies to every grant the client sends, the Authorization Code flow included. The latter refuses a key that disagrees with what the registration approved: the `token_endpoint_auth_signing_alg`, which pins the algorithm for that one client and is narrower than the server-wide list an assertion is checked against, and the `kid` of an inlined `jwks`, which is what the server resolves the assertion's `kid` in. A fresh assertion with a random `jti` is minted per token request, and the algorithm is checked against `token_endpoint_auth_signing_alg_values_supported` when the server advertises one. Symmetric algorithms are refused: an HMAC secret the server already holds proves nothing about who signed.
* Public key publication for the above: `PublicJwk` and `JwkSet` in `volga-oauth-core` (RFC 7517), plus `PrivateKeyJwt::with_public_jwk` / `jwks()`, which fill `kid` and `alg` in from the signing configuration so the published document agrees with what the assertions actually carry - it names a `kid` exactly when the assertions do, adopting one declared on the JWK when the key has none of its own. `PublicJwk` models public signing material exclusively - there is no way to represent `d`, `p`, `q` or the other private members, and deserialization refuses a document carrying them rather than quietly dropping them. It likewise refuses one declaring a use the type cannot serve: a `use` other than `sig`, or a `key_ops` excluding `verify` (RFC 7517 Section 4.3) - dropping such a restriction and republishing the key under `use: "sig"` would turn an encryption-only key into a verification one, and it is the same check `volga` already applies when reading an issuer's JWKS. Combinations no verifier could act on are refused as well: the curve has to belong to the key type (`EcCurve` and `OkpCurve` are separate, so `"kty": "EC"` cannot claim an Edwards curve), and `PublicKey::supports` pins each algorithm to the material that carries it per RFC 7518 Section 3.1 - an RSA key declaring `ES256`, a P-384 key declaring `ES256`, or any public key declaring an HMAC algorithm is rejected by `PublicJwk::with_algorithm`, by deserialization, and by `with_public_jwk` when it disagrees with what the assertions are signed with.
* `jwk::UnsupportedAlgorithm` - the error the checks above return, with `From` conversions into both `volga::Error` (`500`: pairing an algorithm with material that cannot carry it is a misconfiguration of the server, never something a request caused) and `ClientError::Signing`, so either side can propagate it with `?`.
* DPoP sender-constrained tokens (RFC 9449, `volga-oauth-client` feature `dpop`) (#213) - a bearer token is a password: whoever holds it may use it. DPoP binds one to a key the client holds, and every request carries a freshly signed proof of possession, so a stolen token is worth nothing without the key. `Dpop` is that key plus the nonce state of the servers it talks to: `Dpop::generate` mints a throwaway `ES256` one (`generate_with` also does `ES384` and `EdDSA`) - the usual lifetime is one per session, not one per process - while `Dpop::from_pem` / `from_pem_file` adopt a key that outlives it, taking the public half explicitly and checking the two halves against each other with one signature at construction, so a mismatched pair is refused there rather than failing remotely on every request it ever signs. Cloning shares the key and the nonces, so the client and the code making resource requests with its tokens stay in step. Off by default for the same reason as `private-key-jwt` (both need the JWS backend) and independent of it: a client credential says who asked for a token, a DPoP proof says who holds it, and one request may carry both.
* `OAuthClient::with_dpop` puts a proof on every token request, whichever grant sends it. The algorithm is checked against `dpop_signing_alg_values_supported` before the request leaves; the authorization request names the key in `dpop_jkt` (RFC 9449 Section 10), binding the code to it so a stolen one cannot be redeemed by anyone else; and a `use_dpop_nonce` refusal (Section 8.2) is answered by repeating the request exactly once, carrying the nonce that refusal demanded - a fresh request rather than a resend of the first one's bytes, since a `private_key_jwt` assertion is one-shot and a server recording its `jti` would refuse a second sight of it. Nonces are remembered per origin *and* per namespace: a token endpoint and a protected resource issue unrelated sequences (Sections 8 and 9) even when one host serves both. The tokens come back as `token_type: DPoP`; one that does not is refused rather than handed to the caller and the token store as an unbound credential, since a server without DPoP support simply ignores the proof. Token exchange is held to the same rule only where it issues an access token.
* The proof (RFC 9449 Section 4.2) carries `typ: dpop+jwt`, the public key by value in the `jwk` header - never by `kid`, unlike a client assertion - and the `htm` / `htu` / `iat` / `jti` claims, plus `ath`, the hash of the access token, on every request that presents one; `htu` drops the query and fragment. Signing is per request and on the hot path, so the JOSE header, which never varies for a key, is rendered and encoded once when the `Dpop` is built.
* Resource requests stay the caller's: this crate mints proofs and owns the nonce state, it does not become an HTTP client for them. `Dpop::authorize(&mut headers, &method, url, &tokens)` fills in the `Authorization: DPoP <token>` credential and the `DPoP` proof covering it, and reports the nonce that proof carried - resolving and signing in one step, so the answer names what the request actually sent. `Dpop::accept_nonce(url, response.headers())` adopts the nonce of any response and returns it, and `authorize_with_nonce` puts that one in the retry, whatever the shared state has moved on to in the meantime. `Dpop::proof(&method, url)` builds a proof by hand, and `Dpop::thumbprint` is the `jkt` an authorization server binds the token to (Section 6). A token this key cannot present is refused before the request rather than by the resource on every one: a bearer token, or one whose recorded binding names a different key.
* `PublicJwk::thumbprint_input` (`volga-oauth-core`, RFC 7638 Section 3.1) - the canonical JSON a JWK Thumbprint is computed over: the required members of the key type and nothing else, no whitespace, members in lexicographic order. The digest is left to the caller, since this crate does no cryptography. It is what identifies a key across the protocol, and a DPoP proof header's `jwk` and the `jkt` it hashes to are the same rendering, so the two cannot disagree. `EcCurve::as_str` and `OkpCurve::as_str` name the registered `crv` values it is built from.
* `TokenSet::is_dpop` and `TokenSet::dpop_jkt` - whether an issued access token is DPoP-bound (RFC 9449 Section 5), and the thumbprint of the key it is bound to, recorded by the client that obtained it. A `TokenStore` outlives a process while a generated key does not, so `OAuthClient::token` and `ClientCredentialsRequest::token` check a stored entry against the key in hand before serving it: one bound to a key this client cannot prove possession of is dead weight however unexpired it looks, and a bearer entry cached before a key was configured would walk past the refusal above. Neither is an error - it is a stale cache, so the entry is evicted and a token that fits is obtained instead. A bound token is refused by a client with no key whatever the build, a store being shared and outliving any one deployment.
* `OAuthErrorCode::UseDpopNonce` and `OAuthErrorCode::InvalidDpopProof` (RFC 9449 Section 7.1) - the two registered DPoP codes, which used to surface as `Other`. Both take the `400` default of `status()`, which is what a token endpoint answers with; a resource server returns the same codes with `401` and the challenge in `WWW-Authenticate`, so the status there is chosen by the endpoint rather than by the code.
* `From<ClientError> for volga::Error` (feature `oauth-client`) - a handler that talks to an authorization server can now propagate the failure with `?`. The status describes where the failure sits rather than echoing what the authorization server answered, since this application was the *client* of the call that failed: `503` when the server could not be reached, `502` when it answered unusably (a protocol error, an unexpected status, an unparseable body), and `500` for this application's own configuration. A handler that wants to surface the authorization server's own error code matches on `ClientError::Protocol` instead.
* `ClientMetadata::token_endpoint_auth_signing_alg` (`volga-oauth-core`, OpenID Connect Dynamic Client Registration Section 2) with the `with_token_endpoint_auth_signing_alg` builder - first-class instead of an `additional_fields` entry, now that the client checks a signing key against it. Kept as a string so a registration naming an algorithm this framework does not implement still deserializes.
* `volga_oauth_core::protocol` - the registered wire identifiers both sides of the protocol agree on, as `grant`, `client_auth`, `token_type` and `auth_scheme` constants, re-exported from `volga::auth::oauth` and `volga_oauth_client`. A server advertises them in its metadata document and a client matches on them; they live in one place so the two cannot drift.
* `volga_oauth_core::JwsAlgorithm` - the JWS `alg` names shared by everything in the framework that signs or verifies a JWT, with `as_str`, `Display`, `is_symmetric` and serde support. Also re-exported from `volga::auth::oauth`, so it can be named without the `jwt-auth` feature that gates `volga::auth::Algorithm`.
* `volga_oauth_core::jwk` and `volga_oauth_core::pem` - public JWK models and PEM header inspection, both usable from the server side to publish or load keys.
* `AuthorizationServerMetadata::dpop_signing_alg_values_supported` (RFC 9449 Section 5.1) with the `with_dpop_signing_algs` builder, alongside the resource-side field that was already modeled.
* `BearerChallenge::parse_scheme` - reads the challenge for any auth scheme, `parse` being this method with `auth_scheme::BEARER`. Pass `auth_scheme::DPOP` to read what a DPoP-protected resource answers with (RFC 9449 Section 7.1), whose `error` and `error_description` are the RFC 6750 ones. `with_scheme` / `scheme` render and report it, so a parsed challenge re-renders under the scheme it arrived with.
* `ClientError::Signing` - the signing configuration cannot produce the JWS a request needs (a `private_key_jwt` client assertion or a DPoP proof): the key failed to load, the key and the algorithm do not match, or the signature could not be computed. Renders as `JWS signing failed: ..`, naming neither of the two now that it covers both.

## Changed
* Client authentication is checked against `token_endpoint_auth_methods_supported` before a token request leaves (`volga-oauth-client`). Sending a method the authorization server never announced only earns an `invalid_client` over the network, so `client_secret_basic`, `client_secret_post` and `private_key_jwt` are each refused locally when the metadata lists methods and not that one. Metadata listing none is not second-guessed - that is what a hand-built `AuthorizationServerMetadata` carries - but a *discovered* document always lists something, since RFC 8414 Section 2 makes an omitted field mean `client_secret_basic` and deserialization materializes it. A public client presents no credential and is never checked.
* `volga::auth::Algorithm` is now a re-export of `volga_oauth_core::JwsAlgorithm`. The variants, the `HS256` default and the behavior are unchanged, and `jsonwebtoken` stays out of the public API - the mapping onto it moved from an inherent method to a crate-private free function. The type is now shared with the client crates, so a `private_key_jwt` assertion and a bearer token the server issues are described in one vocabulary.
* `EncodingKey` and `DecodingKey` are generated from a single implementation instead of two near-identical copies (about 250 duplicated lines, plus two parallel test suites). The public API - every `from_*` / `try_from_*` constructor, the redacted `Debug` - is unchanged.
* `volga-oauth-client`'s internal transport carries an arbitrary `HeaderMap` on a request and hands the response back whole (status, headers, body) for the caller to judge, instead of one optional `Authorization` header and a parsed body. Nothing public changed; this is what lets a flow read a header off a *failed* response, which the DPoP nonce round needs - the nonce to retry with arrives on the refusal itself.
* The key loading shared by `private_key_jwt` assertions and DPoP proofs - the algorithm-to-family mapping, the PEM header check, the DER path - lives in one place instead of being owned by whichever feature was written first. Nothing public changed, beyond the message for a symmetric algorithm no longer naming `private_key_jwt` specifically.
* `AuthorizationServerMetadata`'s RFC 8414 defaults, the `[oauth.server]` config prefill and the client's `token_endpoint_auth_method` matching all read from the shared `protocol` constants rather than repeating string literals.

## Breaking Changes
* `OAuthErrorCode::from("use_dpop_nonce")` and `from("invalid_dpop_proof")` now yield the new `UseDpopNonce` / `InvalidDpopProof` variants instead of `Other(..)`. The enum is `#[non_exhaustive]`, so nothing stops compiling, but code matching those codes as `Other` no longer matches them; the wire form, `as_str` and the serde representation are unchanged.
* `TokenSet` gained the `dpop_jkt` field, so code constructing one with a struct literal has to name it (`None` for a bearer token). The wire form is unchanged for anything that does not carry a binding - the member is skipped when absent and defaulted when missing - so a persisted entry written by an earlier version still reads back.
* `ClientAuthMethod` (`volga-oauth-client`) is no longer `Copy` - the new `PrivateKeyJwt` variant (feature `private-key-jwt`) carries a signing key. It stays `Clone`, `Debug`, `PartialEq` and `Eq`; two `PrivateKeyJwt` values compare equal when they were built from the same key handle and carry the same claims policy (key material is never compared).

# 0.9.7

## Security
* `App::bind` no longer replaces an address it cannot parse with a default one. A bind string that `parse::<SocketAddr>()` rejected - including `localhost:3000` and the unbracketed `::1:3000`, both of which `std` itself accepts - was silently swapped for `0.0.0.0:7878` on non-Windows targets, so a bind meant to keep a server on loopback listened on every interface with no error and no log line. Such an address is now reported as an `io::Error` from `App::run` (and logged, with no server started, by `App::run_blocking`) (#210).

## Added
* `App::bind` accepts the full socket address grammar of `tokio::net::TcpListener::bind`: host names (`localhost:7878`), unbracketed IPv6 literals (`::1:7878`), zone-scoped IPv6 literals (`[fe80::1%eth0]:7878` - `Ipv6Addr` has no zone id to parse into, so the resolver supplies the scope id), and `SocketAddr` values directly. Names are resolved when the server starts - asynchronously, never blocking the runtime - and a name that resolves to several addresses is tried in resolution order, the first bindable address winning. The `[server] host` config key accepts host names on the same terms (#210).

## Changed
* Bearer authentication against an OAuth issuer (feature `oauth-client`) no longer rebuilds per-request state on every authenticated request. The JWKS key is handed out behind an `Arc` instead of copying (and zeroizing) its key material; the validation policy pinned to the resolved key's algorithm is memoized per algorithm instead of cloning the whole `Validation` (three `HashSet`s plus every audience and issuer string) each time; and the `authorize` middleware resolves the `BearerTokenService` once per request rather than twice. Behavior is unchanged.
* OAuth metadata documents (`use_oauth_resource_metadata`, `use_oauth_server_metadata`, `use_oidc_metadata`) are serialized once when the route is mounted instead of on every request - the documents are immutable, so each response now shares the same buffer.

## Fixed
* The `bump-fuzz-nightly` workflow could never open its monthly PR: `GITHUB_TOKEN` has no `workflows` permission scope, so pushing a branch that edits `.github/workflows/fuzz.yml` was rejected. The pinned nightly moved to `.github/fuzz-nightly`, which the Fuzz workflow reads at run time.

# 0.9.6

## Added
* `ClientMetadata::application_type` (`volga-oauth-core`, RFC 7591 / OIDC Dynamic Client Registration Section 2) with the `with_application_type` builder - first-class instead of an `additional_fields` entry. Desktop and CLI clients register as `"native"`, which is what makes loopback redirect URIs (`http://127.0.0.1:{port}/...`) acceptable to authorization servers.
* `AuthorizationServerMetadata::authorization_response_iss_parameter_supported` (`volga-oauth-core`, RFC 9207 Section 3) with the `with_authorization_response_iss_parameter` builder - likewise typed; it is also accepted in the `[oauth.server]` config file section.
* `AuthorizationRequest::validate_callback` (`volga-oauth-client`) - validates the authorization callback before the code is exchanged: the `state` (CSRF) plus the RFC 9207 `iss`, which must match the issuer whenever present and is required once the server advertises it. Without the `iss` check a callback can be replayed from a different authorization server (mix-up attack); `matches_state` remains for the `state`-only check.

## Fixed
* `OAuthClient::exchange_code`, `refresh` and `token` returned `!Send` futures: the non-`Sync` form serializer was held across the token-endpoint `await`, so the calls could not be spawned onto a multi-thread runtime (callers had to bridge them through a current-thread runtime). The serializer is now dropped before the request; the futures are `Send`, which a test now pins down.

# 0.9.5

## Added
* `oauth-client` feature - issuer-based bearer authentication: `App::with_oauth(|oauth| oauth.with_issuer(..))` plus the explicit `App::use_oauth()` opt-in validate incoming JWTs against the OAuth 2.1/OIDC issuer's published JWKS instead of a static decoding key. Server metadata is discovered per RFC 8414 (with an OIDC Discovery fallback), keys are fetched lazily on the first request and refreshed on `kid` misses (key rotation) behind a configurable cooldown with single-flight; cached keys are re-checked with the issuer once older than a configurable max age (default 15 minutes, `with_max_key_age`), so a revoked or re-keyed `kid` stops validating without a restart while an issuer outage keeps serving the last known set; the `iss` claim is constrained to the configured issuer automatically and made required, so tokens omitting it are rejected. While the issuer is unreachable and no keys are loaded, protected routes answer `503` instead of blaming the token. Everything else (`aud`, expiry, scopes/roles) keeps coming from `with_bearer_auth`.
* `[oauth.client]` config file section (features `oauth-client` + `config`) - describes the issuer (`issuer`, `refresh_cooldown_secs`, `max_key_age_secs`, `require_https`, `timeout_secs`, `max_redirects`) from the configuration file; fields present in the file override `with_oauth` builder calls, unknown keys fail startup, and activation still requires the explicit `App::use_oauth()` call in code.
* `DiscoveryClient::fetch_jwks` / `fetch_jwks_from_url` in `volga-oauth-client` - fetches the issuer's JSON Web Key Set under the shared transport policy; deliberately bypasses the `MetadataCache` (signing keys rotate - freshness policy belongs to the caller).
* New example `oauth_flow` - a complete Authorization Code + PKCE flow between two volga apps: a toy authorization server (metadata, JWKS, `/authorize`, `/token` issuing RS256 tokens) and a resource server protected purely through `use_oauth()`, driven by a `volga-oauth-client` client.
* `oauth` feature (implied by `jwt-auth`) - OAuth 2.1/OIDC foundation at `volga::auth::oauth`: error models (`OAuthError` / `OAuthErrorCode`, covering the registered codes from RFC 6749, 6750, 7591 and 8707), the `WWW-Authenticate` Bearer challenge builder and parser (`BearerChallenge`), resource URI canonicalization and well-known metadata URL derivation.
* OAuth metadata documents and serving: `AuthorizationServerMetadata` (RFC 8414 / OIDC Discovery) and `ProtectedResourceMetadata` (RFC 9728) with builder DSLs. Configure via `App::with_oauth_server_metadata` / `App::with_oauth_resource_metadata` (or the `set_*` counterparts, or the `[oauth.server]` / `[oauth.resource]` config file sections); serve via `App::use_oauth_server_metadata`, `App::use_oauth_resource_metadata` and `App::use_oidc_metadata`.
* Dynamic Client Registration models (RFC 7591): `ClientMetadata` and `ClientRegistrationResponse`.
* New crate `volga-oauth-core` - the protocol-type layer behind `volga::auth::oauth` (no HTTP I/O), shared with the client crate; public `volga` paths are unchanged.
* New crate `volga-oauth-client` - OAuth 2.1/OIDC client independent of the `volga` server crate, usable from any Tokio application (feature flags `http1` (default) / `http2`):
  * `DiscoveryClient` - fetches Authorization Server Metadata (RFC 8414), Protected Resource Metadata (RFC 9728) and the OIDC provider configuration, with the identifier validation the specs require and a `MetadataCache` hook.
  * `OAuthClient` - Authorization Code flow with mandatory PKCE (S256 only), refresh tokens and resource indicators (RFC 8707); token persistence and transparent refresh through the `TokenStore` trait (`InMemoryTokenStore` built in).
  * `RegistrationClient` - Dynamic Client Registration (RFC 7591), including initial access tokens; `OAuthClient::from_registration` adopts the issued credentials.
  * `ClientConfig` transport policy (HTTPS enforcement, total timeouts, redirect limits) and the `ClientError` model shared by all three clients.

## Fixed
* Requests without `Authorization` credentials on a route guarded by `authorize` now answer `401` with a bare `Bearer` challenge (plus `resource_metadata` when configured) per RFC 6750 Section 3, instead of a plain `400` without a challenge - clients can now discover the resource metadata and start an authorization flow. A present but malformed `Authorization` header (wrong scheme, empty token) answers `400` with an `invalid_request` challenge per RFC 6750 Section 3.1; present-but-invalid tokens keep answering `403` with the detailed challenge as before.
* A server built with both `http1` and `http2` (without `ws`) served HTTP/2 exclusively, rejecting HTTP/1 clients even though TLS ALPN advertised `http/1.1`. Such builds now auto-detect the protocol per connection and serve both, matching the `ws` behavior; `http2`-only builds still serve pure HTTP/2.

# 0.9.4

## Added
* HTTP `QUERY` method support: `App::map_query` / `RouteGroup::map_query` register routes for the new verb (#195).
* Generic `App::map` / `RouteGroup::map` - register a route for any HTTP method; accepts anything `TryInto<Method>` (including string verbs like `"QUERY"`) and an owned or borrowed pattern. The named `map_*` helpers are unchanged (#195).
* `HttpBody` is now an extractor - take it directly as a handler argument to access the raw request body (#194).

## Security
* Added a `cargo audit` CI pipeline (#196).
* `jsonwebtoken` switched from the `rust_crypto` backend to `aws_lc_rs`, resolving RUSTSEC-2026-0185 and RUSTSEC-2023-0071 (#196).

# 0.9.3

## Added
* `ShutdownHandle` - programmatic graceful shutdown that composes with the built-in OS signal handler. Construct via `ShutdownHandle::new()` or `ShutdownHandle::from_token(token)` / `From<CancellationToken>`. Trigger with `handle.shutdown()`; observe with `handle.is_shutdown_requested()` and `handle.cancelled()`.
* `App::with_shutdown()` - returns `(App, ShutdownHandle)` for the common case where the framework owns the handle.
* `App::with_shutdown_signal(handle)` - registers an externally-owned `ShutdownHandle` on an existing `App`.
* `App::shutdown_on(future)` - chains async triggers (e.g. an external watchdog future) that fire a graceful shutdown when they resolve. Composes with the OS signal handler and any `ShutdownHandle` already registered, and is safe to call before a Tokio runtime exists.

# 0.9.2

## Added
* `Multipart` is now bidirectional - in addition to acting as a request extractor, it implements `IntoResponse` and can be returned from handlers to produce a `multipart/*` response.
* `Multipart::from_parts(iter)` / `Multipart::from_stream(stream)` - build an outgoing multipart from any `IntoIterator<Item = Part>` or `Stream<Item = Part>`.
* `Multipart::with_subtype(MultipartSubtype)` - switch between `form-data`, `mixed`, `byteranges`, or a `Custom(...)` subtype on outgoing responses.
* `Multipart::with_boundary(...)` - override the auto-generated boundary; validated per RFC 2046 Section 5.1.1.
* `Multipart::into_outgoing()` - re-encode an incoming multipart as a streaming outgoing one for proxy / forwarding scenarios.
* `Part` builder API: `Part::text`, `Part::bytes`, `Part::file`, `Part::stream`, `Part::new`, plus `with_content_type`, `with_disposition`, `with_header_raw`. `Content-Type` is auto-inferred from filename via `mime_guess`. The static-input constructors panic on invalid header bytes; fallible `try_text` / `try_bytes` / `try_file` / `try_stream` / `try_with_disposition` counterparts are provided for untrusted input.
* `OpenApiRouteConfig::produces_multipart(status)` - describe `multipart/form-data` responses in OpenAPI specs.

## Changed
* HSTS default `max_age` is now 1 year (31,536,000 s); previously 30 days. Aligns with the [HSTS preload list](https://hstspreload.org/) requirement (#190).
* `Multipart` request parsing accepts any `multipart/*` subtype (previously only `multipart/form-data`). Required for forwarding `multipart/byteranges`, `multipart/mixed`, etc.

## Breaking Changes
* `HstsConfig::with_preload()` panics if `max_age < 1 year`; `HstsConfig::with_max_age(...)` panics if called when `preload` is enabled and the new value is below 1 year (#190).
* `TlsConfig`, `RedirectionConfig`, and `Problem` are now `#[non_exhaustive]`. External code can no longer construct them with struct literals or exhaustively pattern-match (#190, #191).
* Removed the deprecated `problem!` macro. Use `volga::error::Problem` instead (#191).
* `From<Algorithm> for jsonwebtoken::Algorithm` and the reverse impl are removed. `jsonwebtoken::Algorithm` is no longer reachable through volga's public API; conversion is crate-internal via `Algorithm::to_jwt()` (#191).
* `Problem` responses now use the correct `application/problem+json` content type (#191).

# 0.9.1

## Added
* `EncodingKey::{from_env, try_from_env, from_env_base64, try_from_env_base64, from_file, try_from_file, from_pem_file, try_from_pem_file}` and identical siblings on `DecodingKey` - ergonomic startup-time constructors. Panicking variants expect to be called once at startup; `try_*` variants return `Result<_, volga::Error>`.
* `BearerAuthConfig::with_resource(uri)` / `with_resources(iter)` - OAuth 2.0 resource indicators (RFC 8707).
* `BearerAuthConfig::with_resource_metadata_url(url)` - advertises the OAuth 2.0 Protected Resource Metadata URL (RFC 9728) in `WWW-Authenticate` challenges.
* `BearerAuthConfig::with_strict_aud()` / `BearerAuthConfig::without_strict_aud()` - explicit control over whether `aud` is required when audiences are configured.
* `BearerAuthConfig::strip_token_from_request(bool)` - controls stripping of the `Authorization` header after successful bearer auth.
* `BearerAuthConfig::require_https(bool)` - controls HTTPS enforcement (with loopback exception).
* `CorsConfig::without_credentials()` / `without_vary_header()` - explicit "off" builders paired with the existing `with_*` setters.
* `HstsConfig::without_preload()` / `without_sub_domains()` - explicit "off" builders paired with the existing `with_*` setters.
* `WebSocketConnection::without_accept_unmasked_frames()` - explicit opt-out paired with `with_accept_unmasked_frames()`.

## Breaking Changes
* `volga::auth` no longer re-exports `jsonwebtoken::Algorithm`, `DecodingKey`, `EncodingKey`, `JwtError`, or `ErrorKind`. Replaced by volga-owned `Algorithm`, `DecodingKey`, and `EncodingKey` at the same paths. User code that imports these by name continues to compile; code using `ErrorKind` for pattern-matching JWT errors or calling `EncodingKey::from_rsa_der` / `from_ec_der` / `from_ed_der` / `DecodingKey::from_jwk` / `from_rsa_components` will break. Use the dedicated PEM / base64 / secret / env / file constructors instead.
* `BearerTokenService::validation()` is removed. Configure via `BearerAuthConfig`; no introspection is exposed.
* `BearerAuthConfig::with_aud` now automatically adds `aud` to required claims. Tokens missing `aud` are rejected when audiences are configured. Call `without_strict_aud()` to opt out.
* `require_https` is enabled by default. Non-TLS, non-loopback requests are rejected with `400 Bad Request`. Reverse-proxy deployments must call `require_https(false)`.
* `strip_token_from_request` is enabled by default. The `Authorization` header is removed after successful bearer auth. Disable via `strip_token_from_request(false)` if downstream handlers need it.
* `CorsConfig::with_credentials(bool)` and `with_vary_header(bool)` no longer take a `bool`. The no-arg forms enable the feature; use the new `without_credentials()` / `without_vary_header()` to disable.
* `HstsConfig::with_preload(bool)` and `with_sub_domains(bool)` no longer take a `bool`. The no-arg forms enable the feature; use the new `without_preload()` / `without_sub_domains()` to disable.
* `WebSocketConnection::with_accept_unmasked_frames(bool)` no longer takes a `bool`. Use the no-arg form to enable and `without_accept_unmasked_frames()` to disable.
* Removed `App::with_default_cors()`. Use `.set_cors(CorsConfig::default())` instead.
* Removed `App::with_default_tracing()`. Use `.set_tracing(TracingConfig::default())` instead.
* Removed `TlsConfig::with_hsts_preload`, `with_hsts_sub_domains`, `with_hsts_max_age`, and `with_hsts_exclude_hosts` shortcuts. Configure through the `with_hsts(|h| h. ...)` closure on `TlsConfig` (e.g. `with_hsts(|h| h.with_preload().with_sub_domains())`).

# 0.9.0

## Added
* Added `#[non_exhaustive]` for `Authorizer<C>`, `Encoding`, `WsEvent<T>`
* Added `TracingConfig::without_header()` that disables tracing HTTP header

## Changed
* `App::with_max_header_list_size(Limit::Unlimited)` now always panics as misconfiguration.
* Security defaults changed

## Fixed
* `RouteGroup::cors` now correctly set `CorsOverride::Inherit` instead of disabling it.
* Updated stale MSRV in lib.rs
* Updated crate description for `volga-rate-limiter`

## Breaking Changes
* Header mutation methods now return `&mut Self` (was `Header<T>`/`()`).
* `append_header()` is now infallible and no longer returns Result.
* Changed visibility of `RESPONSE_ERROR` and `SERVER_NAME` constants.
* Changed visibility of `Error::status` and `Error::instance` fields, now these data can be fetched by methods: `Error::status()`, `Error::instance()`

# 0.8.9

## Added
* New `attach()` method for parameterized generic middleware registration (#175)
* New `Filter` trait for parameterized filter middleware (#175)

## Changed
* All middleware registration methods (e.g., `filter()`, `map_ok`, etc) are now allowed to register a parameterized middleware (#175)
* `filter()` middleware now can be registered globally (#175)
* CORS, JWT auth and rate limiting refactored as parameterized middleware (#175)

## Breaking Changes
* Refactored `MiddlewareHandler` trait: removed `type Future`; renamed to `With`; `call()` renamed to `with()` (#175)
* Refactored `TapReqHandler` trait: removed `type Future`; renamed to `TapReq`; `call()` renamed to `tap_req()` (#175)
* Refactored `MapOkHandler` trait: removed `type Future`; renamed to `MapOk`; `call()` renamed to `map_ok()` (#175)
* Refactored `MapErrHandler` trait: removed `type Future`; renamed to `MapErr`; `call()` renamed to `map_err()` (#175)

# 0.8.8

## Added
* Added the ability to configure server from a file (#173)

## 0.8.7

## Added
* Added `to_map()` method in `HttpHeader` struct (#169)
* Added rustfmt formatting check to CI (#170)
* Exposed greeter for release builds (requires explicit enabling) (#171)
* Added traits for custom storage implementations for rate limiters (#171)

## Fixed
* Fixed formatting across the project (#170)
* Greeter now respects `NO_COLOR` env var (#171)

## 0.8.6

### Added
* fuzz tests for router and OpenAPI (#166)

## Changed
* Added security notes for tap_req middleware (#167)
* Added safety notes for wrap middleware (#167)
* Improved performance of the entire middleware pipeline, reducing heap allocations (#167)
* Unused Next/NextFn are now zero-alloc (#167)
* Refactored directory listing HTML generation. (#165)
* Removed dependencies on `handlebars` and `chrono` (#165)

## 0.8.5

### Added
* Per-status-code OpenAPI response config: `produces_*` methods now accept a status code, `IntoStatusCode` trait (supports `u16`, `u32`, `i32`, `http::StatusCode`) (#162)
* OpenAPI `produces_problem()` and `produces_problem_example()` for `application/problem+json` responses, gated on `problem-details` feature (#162)

### Changed
* Nested Route Groups support with middleware/CORS/OpenAPI isolation (#164)
* Updated Global Error Handler: improved performance at the request hot-path (#163)

## 0.8.4

### Added
* Open API integration (#159)

## 0.8.3

### Added
* New async stream macro, helpers and extractors (#155)

### Changed
* WebSocket improvements (#153)
* SSE Improvements (#154)
* SSE improvements + relaxed Sync requirements for middleware and handers (#156)

## 0.8.2

### Added
- Added ability to override TCP Listener (#149)
- Add Token Bucket and GCRA rate limiting algorithms (#152)

### Changed
- `HEAD` request handling improvements (#150) 
- `FromPayload` improvements (#151)

## 0.8.1

### Changed
- HTTP/RFC compliance (#145)
- `HttpBody` improvements (#146)
- Performance Improvements (#147)
-  Security Improvements (#148)

## 0.8.0

### Added
- Rate Limiting (#132)
- Added `accepted` and `created` macros (#131)

### Changed
- Backpressure & limits (#135)
- Refactor Cache-Control, HSTS and Tracing (#136)
- Improve CORS Middleware: Correct Preflight Handling & Precomputed Headers (#137)

### Tests
- Improvements for integration tests (#133)

## 0.7.3

### Changed
- Updated dependencies (#129)
- Problem details updates (#130)

### Documentation
- Corrected docs (#128)

## 0.7.2

### Changed
- Updated crates metadata structure (#127)

### Performance
- Routing performance improvements (#127)

## 0.7.1

### Changed
- HttpRequest improvements for middlewares (#126)
- Small adjustments (#125)

## 0.7.0

### Changed
- Migration to Rust 1.90 (#123)
- DI refactoring and improvements (#124)

## 0.6.7

### Performance
- Routing and Middleware performance improvements (#122)

## 0.6.6

### Documentation
- Updated readmes (#121)

## 0.6.5

### Added
- Self-signed dev cert generation for local development (#120)

## 0.6.4

### Changed
- Type extractors improvements (#119)

## 0.6.3

### Fixed
- Fixed issue with versions of internal dependencies (#117)

## 0.6.2

### Changed
- Fallback and Tracing improvements (#115)
- Moved DI tools into a separate crate (#113)

### Documentation
- Updated readme and dependencies (#116)

## 0.6.1

### Added
- Additional middleware (#112)

## 0.6.0

### Added
- Authorization and Authentication tools (#110)
- Added new welcome screen in debug mode (#108)
- Route filters and middlewares (#106)
- Added the ability to read signed key and private key from a file (#105)
- Private and Signed cookies (#102)
- Added Cookies feature to work with cookies (#101)
- CORS (#95)
- Added `set_key`, `set_cert` and `set_pem` methods to configure TLS (#92)
- Initial WebSockets implementation (#82)
- Serving static files (#77)
- Customizable fallback handler and HTML responses (#75)
- Added configurable request body limit (5 MB default) (#68)
- Added the `problem!` macro for Problem Details responses (#64)
- Added basic benchmark and global error handler (#63)
- Added tracing example (#61)
- Implemented graceful shutdown (#60)
- Opt-in HSTS middleware (#58)
- HTTPS redirection (#57)
- TLS support (#56)

### Changed
- Doc, run_blocking and some improvements (#109)
- Middleware improvements (#107)
- Additional enhancement for SSE messages (#100)
- Additional SSE improvements (#99)
- SSE, stream response improvements (#98)
- Changed design of `with_tls`, `with_tracing`, `with_host_env` and `with_hsts` methods (#91)
- Websocket splitting improvements (#89)
- Feature/perf improvements and more tests (#87)
- Improved DI with WebSockets/WebTransport (#84)
- WebSockets & WebTransport improvements (#83)
- TLS, tracing and static files improvements (#79)
- Extractors improvements (#76)
- Ongoing DI improvements (#72)
- Added usage of `resolve_ref()` across `HttpContext` and `HttpRequest` (#71)
- DI container optimizations, ability to resolve as ref (#70)
- DI scoped service resolution improvements (#69)
- Replaced `std::io::Error` with custom, more specific `Error` type (#65)
- HTTP Response improvements (#54)
- Version increase (#74)

### Fixed
- Several fixes for static files serving and WebSocket connection validation (#85)
- Small tweaks for static files serving logic (#78)
- Fixed unstable unit test (#59)

### Performance
- Routing performance improvements (#86)
- DI container optimizations (#73)

### Tests
- Added coverage check + more tests + more docs (#96)
- Added more unit tests for extractors (#94)
- More Unit Tests + small fixes (#93)
- Added additional Unit & Integration Tests (#90)
- Added more unit tests for TLS, DI and error handling logic (#88)
- Additional Unit Tests and improvements (#81)

### Documentation
- Readme updates (#103)

## 0.5.0

### Added
- Multipart/form-data extractor (#53)
- Added `Form<T>` Form Data extractor (#52)

### Changed
- Updated version (#55)
- HTTP Response improvements (#54)
