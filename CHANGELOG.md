# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/).

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
