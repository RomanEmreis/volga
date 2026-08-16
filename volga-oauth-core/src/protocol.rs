//! Registered OAuth 2.0 protocol identifiers
//!
//! The wire strings both sides of the protocol agree on: an authorization
//! server advertises them in its metadata document
//! ([`AuthorizationServerMetadata`](crate::AuthorizationServerMetadata)), a
//! client sends them in its requests and matches on what it was given. They
//! live here so the two never drift apart.

/// Grant type identifiers (the `grant_type` request parameter and the
/// `grant_types_supported` metadata field)
pub mod grant {
    /// The authorization code grant (RFC 6749 Section 4.1) - the only
    /// redirect-based flow retained in OAuth 2.1
    pub const AUTHORIZATION_CODE: &str = "authorization_code";

    /// The refresh token grant (RFC 6749 Section 6)
    pub const REFRESH_TOKEN: &str = "refresh_token";

    /// The client credentials grant (RFC 6749 Section 4.4) - the client
    /// authenticates as itself, with no user involved
    pub const CLIENT_CREDENTIALS: &str = "client_credentials";

    /// The implicit grant (RFC 6749 Section 4.2)
    ///
    /// Removed from OAuth 2.1 and never issued by this framework; present
    /// because RFC 8414 Section 2 names it in the default value of
    /// `grant_types_supported`, so clients still have to recognize it.
    pub const IMPLICIT: &str = "implicit";

    /// The JWT bearer authorization grant (RFC 7523 Section 2.1) - a JWT
    /// issued elsewhere is presented as the grant
    pub const JWT_BEARER: &str = "urn:ietf:params:oauth:grant-type:jwt-bearer";

    /// The token exchange grant (RFC 8693) - one token is traded for
    /// another, possibly of a different type
    pub const TOKEN_EXCHANGE: &str = "urn:ietf:params:oauth:grant-type:token-exchange";

    /// Returns whether a registration's `grant_types` covers `grant_type`
    ///
    /// An empty list is not "every grant": RFC 7591 Section 2 defines the
    /// default for an omitted `grant_types` as [`AUTHORIZATION_CODE`]
    /// alone. Both sides of a registration read the field through here, so
    /// the client that sends one and the client built from the response
    /// cannot disagree about what it meant.
    pub fn covers(registered: &[String], grant_type: &str) -> bool {
        match registered {
            [] => grant_type == AUTHORIZATION_CODE,
            registered => registered.iter().any(|grant| grant == grant_type),
        }
    }
}

/// Client authentication method identifiers (the
/// `token_endpoint_auth_method` registration field and the
/// `*_endpoint_auth_methods_supported` metadata fields)
pub mod client_auth {
    /// No client authentication - a public client, identified by
    /// `client_id` alone and protected by PKCE
    pub const NONE: &str = "none";

    /// HTTP Basic authentication with the client secret (RFC 6749
    /// Section 2.3.1) - the default, and the method servers are required to
    /// support
    pub const CLIENT_SECRET_BASIC: &str = "client_secret_basic";

    /// The client secret in the request body (RFC 6749 Section 2.3.1), for
    /// servers that do not accept HTTP Basic authentication
    pub const CLIENT_SECRET_POST: &str = "client_secret_post";

    /// A client assertion signed with the client secret as an HMAC key
    /// (RFC 7523 Section 2.2)
    pub const CLIENT_SECRET_JWT: &str = "client_secret_jwt";

    /// A client assertion signed with the client's own private key
    /// (RFC 7523 Section 2.2), so no shared secret ever leaves the client
    pub const PRIVATE_KEY_JWT: &str = "private_key_jwt";

    /// The `client_assertion_type` accompanying a `client_secret_jwt` or
    /// `private_key_jwt` assertion (RFC 7523 Section 2.2)
    pub const ASSERTION_TYPE_JWT_BEARER: &str =
        "urn:ietf:params:oauth:client-assertion-type:jwt-bearer";
}

/// HTTP authentication scheme names (the `WWW-Authenticate` challenge and
/// the `Authorization` credential that answers it)
pub mod auth_scheme {
    /// The bearer token scheme (RFC 6750) - possession of the token is the
    /// whole credential
    pub const BEARER: &str = "Bearer";

    /// The DPoP scheme (RFC 9449) - the token is bound to a key the client
    /// proves possession of on every request
    pub const DPOP: &str = "DPoP";
}

/// Token type identifiers used by token exchange (RFC 8693 Section 3)
pub mod token_type {
    /// An OAuth 2.0 access token
    pub const ACCESS_TOKEN: &str = "urn:ietf:params:oauth:token-type:access_token";

    /// An OAuth 2.0 refresh token
    pub const REFRESH_TOKEN: &str = "urn:ietf:params:oauth:token-type:refresh_token";

    /// An OpenID Connect ID token
    pub const ID_TOKEN: &str = "urn:ietf:params:oauth:token-type:id_token";

    /// A plain JWT, of no more specific registered type
    pub const JWT: &str = "urn:ietf:params:oauth:token-type:jwt";

    /// An identity assertion authorization grant - the cross-domain
    /// assertion an identity provider issues for an application to present
    /// to a resource's authorization server
    pub const ID_JAG: &str = "urn:ietf:params:oauth:token-type:id-jag";
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_spells_the_registered_urns() {
        // the URN forms are easy to typo and impossible to notice at
        // runtime: a mistyped grant type reads as `unsupported_grant_type`
        assert_eq!(
            grant::JWT_BEARER,
            "urn:ietf:params:oauth:grant-type:jwt-bearer"
        );
        assert_eq!(
            grant::TOKEN_EXCHANGE,
            "urn:ietf:params:oauth:grant-type:token-exchange"
        );
        assert_eq!(
            client_auth::ASSERTION_TYPE_JWT_BEARER,
            "urn:ietf:params:oauth:client-assertion-type:jwt-bearer"
        );
        assert_eq!(
            token_type::ACCESS_TOKEN,
            "urn:ietf:params:oauth:token-type:access_token"
        );
        assert_eq!(
            token_type::ID_JAG,
            "urn:ietf:params:oauth:token-type:id-jag"
        );
    }
}
