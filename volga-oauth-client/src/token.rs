//! Token models
//!
//! [`TokenResponse`] is the wire shape of a successful token endpoint
//! response (RFC 6749 Section 5.1); [`TokenSet`] is what the application holds
//! on to - the same fields with `expires_in` resolved into an absolute
//! [`SystemTime`] captured when the response was received.

use serde::{Deserialize, Serialize};
use std::time::{Duration, SystemTime};

/// A successful token endpoint response (RFC 6749 Section 5.1)
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenResponse {
    /// The issued access token
    pub access_token: String,

    /// The token type, almost always `Bearer` (case-insensitive)
    pub token_type: String,

    /// Access token lifetime in seconds
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_in: Option<u64>,

    /// Refresh token, when the server issued one
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,

    /// The granted scope, when it differs from the requested one
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,

    /// OpenID Connect ID token; passed through as-is, not validated
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id_token: Option<String>,
}

/// Tokens held by the application
///
/// Produced from a [`TokenResponse`] via `From`, which resolves the
/// relative `expires_in` into an absolute [`expires_at`](Self::expires_at).
/// Serializable so a [`TokenStore`](crate::TokenStore) implementation can
/// persist it.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TokenSet {
    /// The access token
    pub access_token: String,

    /// The token type, almost always `Bearer` (case-insensitive)
    pub token_type: String,

    /// Refresh token, when the server issued one
    pub refresh_token: Option<String>,

    /// The granted scope, when the server reported it
    pub scope: Option<String>,

    /// OpenID Connect ID token; passed through as-is, not validated
    pub id_token: Option<String>,

    /// Absolute access token expiration; `None` when the server did not
    /// report a lifetime (or reported one too large to represent)
    pub expires_at: Option<SystemTime>,

    /// The RFC 7638 thumbprint of the key this token is bound to, when it
    /// is DPoP-bound (RFC 9449 Section 6)
    ///
    /// Recorded by the client that obtained it, so a persisted entry can be
    /// told apart from one bound to a key this process no longer holds -
    /// a token nothing can prove possession of is dead weight, however
    /// unexpired it looks. `None` on a bearer token, and on one persisted
    /// before this was recorded.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dpop_jkt: Option<String>,
}

impl TokenSet {
    /// Returns `true` when the access token is DPoP-bound (RFC 9449
    /// Section 5) and must be presented under that scheme
    ///
    /// A DPoP-bound token is not a bearer token: sending it as `Bearer`
    /// gives up the binding and is refused by any server that issued it.
    /// [`Dpop::authorize`](crate::Dpop::authorize) presents it correctly;
    /// this is how a caller doing its own request handling tells the two
    /// apart. `token_type` is case-insensitive per RFC 6749 Section 5.1.
    ///
    /// ```
    /// # use volga_oauth_client::{TokenResponse, TokenSet};
    /// # let response = TokenResponse {
    /// #     access_token: "at".into(),
    /// #     token_type: "DPoP".into(),
    /// #     expires_in: None,
    /// #     refresh_token: None,
    /// #     scope: None,
    /// #     id_token: None,
    /// # };
    /// let tokens = TokenSet::from(response);
    /// assert!(tokens.is_dpop());
    /// ```
    #[inline]
    pub fn is_dpop(&self) -> bool {
        self.token_type
            .eq_ignore_ascii_case(volga_oauth_core::auth_scheme::DPOP)
    }

    /// Returns `true` when the access token has expired
    ///
    /// A token without a known lifetime never reports as expired.
    #[inline]
    pub fn is_expired(&self) -> bool {
        self.expires_within(Duration::ZERO)
    }

    /// Returns `true` when the access token expires within `leeway` from
    /// now (or already has)
    ///
    /// A `leeway` too large to represent covers any expiration.
    pub fn expires_within(&self, leeway: Duration) -> bool {
        self.expires_at.is_some_and(|expires_at| {
            SystemTime::now()
                .checked_add(leeway)
                .is_none_or(|deadline| deadline >= expires_at)
        })
    }
}

impl From<TokenResponse> for TokenSet {
    fn from(response: TokenResponse) -> Self {
        Self {
            access_token: response.access_token,
            token_type: response.token_type,
            refresh_token: response.refresh_token,
            scope: response.scope,
            id_token: response.id_token,
            expires_at: expires_at(response.expires_in),
            // stamped by the client that issued the request, which is what
            // knows the key - see `OAuthClient::adopt_tokens`
            dpop_jkt: None,
        }
    }
}

/// Resolves a token response's relative `expires_in` into an absolute
/// expiration, captured now.
///
/// An `expires_in` too large to represent as a [`SystemTime`] (a buggy or
/// malicious server) is treated as no reported lifetime rather than
/// panicking.
pub(crate) fn expires_at(expires_in: Option<u64>) -> Option<SystemTime> {
    expires_in.and_then(|secs| SystemTime::now().checked_add(Duration::from_secs(secs)))
}

impl std::fmt::Debug for TokenResponse {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // tokens are credentials - never expose them in debug output
        f.debug_struct("TokenResponse")
            .field("access_token", &"[redacted]")
            .field("token_type", &self.token_type)
            .field("expires_in", &self.expires_in)
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "[redacted]"),
            )
            .field("scope", &self.scope)
            .field("id_token", &self.id_token.as_ref().map(|_| "[redacted]"))
            .finish()
    }
}

impl std::fmt::Debug for TokenSet {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TokenSet")
            .field("access_token", &"[redacted]")
            .field("token_type", &self.token_type)
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "[redacted]"),
            )
            .field("scope", &self.scope)
            .field("id_token", &self.id_token.as_ref().map(|_| "[redacted]"))
            .field("expires_at", &self.expires_at)
            // a thumbprint is a public identifier, not a secret
            .field("dpop_jkt", &self.dpop_jkt)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(expires_in: Option<u64>) -> TokenResponse {
        TokenResponse {
            access_token: "at".into(),
            token_type: "Bearer".into(),
            expires_in,
            refresh_token: Some("rt".into()),
            scope: Some("read".into()),
            id_token: None,
        }
    }

    #[test]
    fn it_deserializes_a_minimal_response() {
        let response: TokenResponse =
            serde_json::from_str(r#"{"access_token": "at", "token_type": "Bearer"}"#).unwrap();
        assert_eq!(response.access_token, "at");
        assert_eq!(response.expires_in, None);
        assert_eq!(response.refresh_token, None);
    }

    #[test]
    fn it_resolves_expiration_into_absolute_time() {
        let tokens = TokenSet::from(response(Some(3600)));
        let expires_at = tokens.expires_at.unwrap();
        let lifetime = expires_at.duration_since(SystemTime::now()).unwrap();
        assert!(lifetime > Duration::from_secs(3590) && lifetime <= Duration::from_secs(3600));

        assert!(!tokens.is_expired());
        assert!(tokens.expires_within(Duration::from_secs(3601)));

        // no reported lifetime - never expired
        let tokens = TokenSet::from(response(None));
        assert!(!tokens.is_expired());
        assert!(!tokens.expires_within(Duration::from_secs(3600)));

        let tokens = TokenSet::from(response(Some(0)));
        assert!(tokens.is_expired());
    }

    #[test]
    fn it_survives_unrepresentable_lifetimes() {
        // an overflowing `expires_in` must not panic - it degrades to
        // "no reported lifetime"
        let tokens = TokenSet::from(response(Some(u64::MAX)));
        assert_eq!(tokens.expires_at, None);
        assert!(!tokens.is_expired());

        // an overflowing leeway covers any expiration
        let tokens = TokenSet::from(response(Some(3600)));
        assert!(tokens.expires_within(Duration::MAX));
        let tokens = TokenSet::from(response(None));
        assert!(!tokens.expires_within(Duration::MAX));
    }

    #[test]
    fn it_keeps_an_unrecorded_binding_out_of_the_wire_form() {
        // an entry persisted before the binding was recorded still reads
        // back, and a bearer token does not grow a null member
        let tokens = TokenSet::from(response(Some(60)));
        assert_eq!(tokens.dpop_jkt, None);

        let json = serde_json::to_value(&tokens).unwrap();
        assert!(json.get("dpop_jkt").is_none());
        assert_eq!(serde_json::from_value::<TokenSet>(json).unwrap(), tokens);

        let bound = TokenSet {
            dpop_jkt: Some("jkt".into()),
            ..tokens
        };
        let json = serde_json::to_value(&bound).unwrap();
        assert_eq!(json["dpop_jkt"], "jkt");
        assert_eq!(serde_json::from_value::<TokenSet>(json).unwrap(), bound);
    }

    #[test]
    fn it_recognizes_a_dpop_bound_token() {
        let mut response = response(None);
        assert!(!TokenSet::from(response.clone()).is_dpop());

        // RFC 6749 Section 5.1 makes `token_type` case-insensitive, and
        // servers do spell it every which way
        for spelling in ["DPoP", "dpop", "DPOP"] {
            response.token_type = spelling.into();
            assert!(
                TokenSet::from(response.clone()).is_dpop(),
                "'{spelling}' was not recognized"
            );
        }
    }

    #[test]
    fn it_redacts_tokens_in_debug_output() {
        let debug = format!("{:?}", TokenSet::from(response(Some(60))));
        assert!(!debug.contains("at") || debug.contains("[redacted]"));
        assert!(!debug.contains("\"rt\""));
        let debug = format!("{:?}", response(Some(60)));
        assert!(debug.contains("[redacted]"));
    }
}
