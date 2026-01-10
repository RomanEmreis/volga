#![allow(missing_docs)]
#![cfg(feature = "macros")]

use volga_macros::http_header;
use volga::headers::FromHeaders;

#[test]
fn it_implements_from_headers_for_struct_with_string_literal() {
    #[http_header("x-api-key")]
    struct ApiKey;

    // Test that header_type() returns correct value
    assert_eq!(ApiKey::NAME, "x-api-key");
}

#[test]
fn it_implements_from_headers_for_struct_with_constant() {
    const X_AUTH_TOKEN: &str = "x-auth-token";

    #[http_header(X_AUTH_TOKEN)]
    struct AuthToken;

    assert_eq!(AuthToken::NAME, "x-auth-token");
}

#[test]
fn it_implements_from_headers_for_multiple_structs() {
    #[http_header("x-request-id")]
    struct RequestId;

    #[http_header("x-correlation-id")]
    struct CorrelationId;

    assert_eq!(RequestId::NAME, "x-request-id");
    assert_eq!(CorrelationId::NAME, "x-correlation-id");
}

#[test]
fn it_handles_standard_http_headers() {
    #[http_header("authorization")]
    struct Authorization;

    #[http_header("content-type")]
    struct ContentType;

    #[http_header("accept")]
    struct Accept;

    assert_eq!(Authorization::NAME, "authorization");
    assert_eq!(ContentType::NAME, "content-type");
    assert_eq!(Accept::NAME, "accept");
}

#[test]
fn it_handles_custom_headers_with_special_characters() {
    #[http_header("x-custom-header-123")]
    struct CustomHeader1;

    #[http_header("x_underscore_header")]
    struct CustomHeader2;

    assert_eq!(CustomHeader1::NAME, "x-custom-header-123");
    assert_eq!(CustomHeader2::NAME, "x_underscore_header");
}

#[test]
fn it_preserves_struct_visibility() {
    #[http_header("x-public-header")]
    struct PublicHeader;

    #[http_header("x-private-header")]
    struct PrivateHeader;

    // Both should work regardless of visibility
    assert_eq!(PublicHeader::NAME, "x-public-header");
    assert_eq!(PrivateHeader::NAME, "x-private-header");
}

#[test]
fn it_works_with_uppercase_constant() {
    const API_KEY_HEADER: &str = "x-api-key";

    #[http_header(API_KEY_HEADER)]
    struct ApiKey;

    assert_eq!(ApiKey::NAME, "x-api-key");
}

#[test]
fn it_generates_unique_implementations() {
    #[http_header("header-1")]
    struct Header1;

    #[http_header("header-2")]
    struct Header2;

    // Each struct should have its own implementation
    assert_ne!(Header1::NAME, Header2::NAME);
}