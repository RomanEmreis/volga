# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/).

# 0.9.2

## Added
* `Multipart` is now bidirectional — in addition to acting as a request extractor, it implements `IntoResponse` and can be returned from handlers to produce a `multipart/*` response.
* `Multipart::from_parts(iter)` / `Multipart::from_stream(stream)` — build an outgoing multipart from any `IntoIterator<Item = Part>` or `Stream<Item = Part>`.
* `Multipart::with_subtype(MultipartSubtype)` — switch between `form-data`, `mixed`, `byteranges`, or a `Custom(...)` subtype on outgoing responses.
* `Multipart::with_boundary(...)` — override the auto-generated boundary; validated per RFC 2046 §5.1.1.
* `Multipart::into_outgoing()` — re-encode an incoming multipart as a streaming outgoing one for proxy / forwarding scenarios.
* `Part` builder API: `Part::text`, `Part::bytes`, `Part::file`, `Part::stream`, `Part::new`, plus `with_content_type`, `with_disposition`, `with_header_raw`. `Content-Type` is auto-inferred from filename via `mime_guess`. The static-input constructors panic on invalid header bytes; fallible `try_text` / `try_bytes` / `try_file` / `try_stream` / `try_with_disposition` counterparts are provided for untrusted input.
* `OpenApiRouteConfig::produces_multipart(status)` — describe `multipart/form-data` responses in OpenAPI specs.

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
* `EncodingKey::{from_env, try_from_env, from_env_base64, try_from_env_base64, from_file, try_from_file, from_pem_file, try_from_pem_file}` and identical siblings on `DecodingKey` — ergonomic startup-time constructors. Panicking variants expect to be called once at startup; `try_*` variants return `Result<_, volga::Error>`.
* `BearerAuthConfig::with_resource(uri)` / `with_resources(iter)` — OAuth 2.0 resource indicators (RFC 8707).
* `BearerAuthConfig::with_resource_metadata_url(url)` — advertises the OAuth 2.0 Protected Resource Metadata URL (RFC 9728) in `WWW-Authenticate` challenges.
* `BearerAuthConfig::with_strict_aud()` / `BearerAuthConfig::without_strict_aud()` — explicit control over whether `aud` is required when audiences are configured.
* `BearerAuthConfig::strip_token_from_request(bool)` — controls stripping of the `Authorization` header after successful bearer auth.
* `BearerAuthConfig::require_https(bool)` — controls HTTPS enforcement (with loopback exception).
* `CorsConfig::without_credentials()` / `without_vary_header()` — explicit "off" builders paired with the existing `with_*` setters.
* `HstsConfig::without_preload()` / `without_sub_domains()` — explicit "off" builders paired with the existing `with_*` setters.
* `WebSocketConnection::without_accept_unmasked_frames()` — explicit opt-out paired with `with_accept_unmasked_frames()`.

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
