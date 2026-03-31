# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/).

# Unreleased

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
