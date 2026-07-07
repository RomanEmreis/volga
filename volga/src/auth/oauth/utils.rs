//! Shared OAuth utilities
//!
//! * [`BearerChallenge`] — builder for `WWW-Authenticate: Bearer ...`
//!   challenges per [RFC 6750 §3](https://www.rfc-editor.org/rfc/rfc6750#section-3)
//!   and [RFC 9728 §5.1](https://www.rfc-editor.org/rfc/rfc9728#section-5.1)
//! * [`canonicalize_resource_uri`] — resource indicator normalization per
//!   [RFC 8707 §2](https://www.rfc-editor.org/rfc/rfc8707#section-2)

use super::error::{OAuthError, OAuthErrorCode};
use std::fmt::{self, Display, Formatter, Write};
use std::net::Ipv6Addr;

/// Builder for a `WWW-Authenticate: Bearer` challenge header value
///
/// Parameters are emitted in a stable order: `realm`, `error`,
/// `error_description`, `scope`, `resource_metadata`. All values are
/// quoted; embedded `"` and `\` are escaped and control characters are
/// replaced with spaces so the result is always a valid header value.
///
/// # Example
/// ```
/// use volga::auth::oauth::{BearerChallenge, OAuthErrorCode};
///
/// let challenge = BearerChallenge::new()
///     .with_error(OAuthErrorCode::InvalidToken)
///     .with_description("Token has expired")
///     .to_string();
///
/// assert_eq!(
///     challenge,
///     r#"Bearer error="invalid_token", error_description="Token has expired""#
/// );
/// ```
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BearerChallenge {
    realm: Option<String>,
    error: Option<OAuthErrorCode>,
    error_description: Option<String>,
    scope: Option<String>,
    resource_metadata: Option<String>,
}

impl BearerChallenge {
    /// Creates an empty challenge (renders as `Bearer`)
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the `realm` parameter
    pub fn with_realm(mut self, realm: impl Into<String>) -> Self {
        self.realm = Some(realm.into());
        self
    }

    /// Sets the `error` parameter (RFC 6750 §3.1)
    pub fn with_error(mut self, error: OAuthErrorCode) -> Self {
        self.error = Some(error);
        self
    }

    /// Sets the `error_description` parameter
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.error_description = Some(description.into());
        self
    }

    /// Sets the `scope` parameter listing the scopes required to access the resource
    pub fn with_scope(mut self, scope: impl Into<String>) -> Self {
        self.scope = Some(scope.into());
        self
    }

    /// Sets the `resource_metadata` parameter pointing to the protected
    /// resource metadata document (RFC 9728 §5.1)
    pub fn with_resource_metadata(mut self, url: impl Into<String>) -> Self {
        self.resource_metadata = Some(url.into());
        self
    }
}

impl Display for BearerChallenge {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        f.write_str("Bearer")?;
        let mut first = true;
        if let Some(realm) = &self.realm {
            write_param(f, &mut first, "realm", realm)?;
        }
        if let Some(error) = &self.error {
            write_param(f, &mut first, "error", error.as_str())?;
        }
        if let Some(description) = &self.error_description {
            write_param(f, &mut first, "error_description", description)?;
        }
        if let Some(scope) = &self.scope {
            write_param(f, &mut first, "scope", scope)?;
        }
        if let Some(url) = &self.resource_metadata {
            write_param(f, &mut first, "resource_metadata", url)?;
        }
        Ok(())
    }
}

/// Writes a single `name="value"` auth parameter, escaping the value as an
/// RFC 7235 quoted-string and replacing control characters with spaces.
fn write_param(f: &mut Formatter<'_>, first: &mut bool, name: &str, value: &str) -> fmt::Result {
    if *first {
        f.write_char(' ')?;
        *first = false;
    } else {
        f.write_str(", ")?;
    }
    f.write_str(name)?;
    f.write_str("=\"")?;
    for symbol in value.chars() {
        match symbol {
            '"' | '\\' => {
                f.write_char('\\')?;
                f.write_char(symbol)?;
            }
            symbol if symbol.is_control() => f.write_char(' ')?,
            symbol => f.write_char(symbol)?,
        }
    }
    f.write_char('"')
}

/// Canonicalizes an OAuth 2.0 resource indicator (RFC 8707) so that
/// equivalent URIs compare equal as strings (e.g. for `aud` matching).
///
/// Normalization applied:
/// * the scheme and host are lowercased;
/// * default ports are removed (`http`/`ws`: 80, `https`/`wss`: 443);
/// * a lone root path (`https://example.com/`) is dropped
///   (`https://example.com`); non-root paths and query strings are preserved.
///
/// Returns an [`OAuthError`] with code `invalid_target` when the URI is not
/// an absolute URI, contains a fragment, userinfo, whitespace, control or
/// non-ASCII characters, uses a web scheme (`http`, `https`, `ws`, `wss`)
/// without an authority (`https:api.example.com`), or has a bracketed host
/// that is not a valid IPv6/IPvFuture literal. Percent-encoding and dot-segment normalization are
/// not performed.
///
/// # Example
/// ```
/// use volga::auth::oauth::canonicalize_resource_uri;
///
/// let uri = canonicalize_resource_uri("HTTPS://API.Example.COM:443/v1").unwrap();
/// assert_eq!(uri, "https://api.example.com/v1");
/// ```
pub fn canonicalize_resource_uri(uri: &str) -> Result<String, OAuthError> {
    if uri.is_empty() {
        return Err(invalid_target("resource URI must not be empty"));
    }
    if uri.bytes().any(|b| !(0x21..=0x7e).contains(&b)) {
        return Err(invalid_target(
            "resource URI must not contain whitespace, control or non-ASCII characters",
        ));
    }
    if uri.contains('#') {
        return Err(invalid_target("resource URI must not contain a fragment"));
    }
    let Some((scheme, rest)) = uri.split_once(':') else {
        return Err(invalid_target("resource URI must be an absolute URI"));
    };
    let valid_scheme = scheme
        .chars()
        .next()
        .is_some_and(|c| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'));
    if !valid_scheme {
        return Err(invalid_target("resource URI scheme is invalid"));
    }
    let scheme = scheme.to_ascii_lowercase();

    let Some(after_scheme) = rest.strip_prefix("//") else {
        // Web schemes always carry an authority: `https:api.example.com`
        // or `https:/api` is a mistyped resource, not a URN-style URI
        if matches!(scheme.as_str(), "http" | "https" | "ws" | "wss") {
            return Err(invalid_target("resource URI must have an authority"));
        }
        // No authority component (e.g. `urn:example:resource`) —
        // only the scheme is subject to normalization
        return Ok(format!("{scheme}:{rest}"));
    };

    let authority_end = after_scheme.find(['/', '?']).unwrap_or(after_scheme.len());
    let (authority, path_and_query) = after_scheme.split_at(authority_end);
    if authority.contains('@') {
        return Err(invalid_target("resource URI must not contain userinfo"));
    }
    let (host, port) = split_host_port(authority)?;
    if host.is_empty() {
        return Err(invalid_target("resource URI must have a host"));
    }
    let host = host.to_ascii_lowercase();

    let port = match port {
        // An empty port (`https://example.com:`) is dropped
        None | Some("") => None,
        Some(port) => {
            if !port.bytes().all(|b| b.is_ascii_digit()) {
                return Err(invalid_target("resource URI port is invalid"));
            }
            match (scheme.as_str(), port) {
                ("http" | "ws", "80") | ("https" | "wss", "443") => None,
                _ => Some(port),
            }
        }
    };

    let mut result = format!("{scheme}://{host}");
    if let Some(port) = port {
        result.push(':');
        result.push_str(port);
    }
    if path_and_query != "/" {
        result.push_str(path_and_query);
    }
    Ok(result)
}

/// Splits a URI authority (without userinfo) into host and optional port,
/// keeping IP literals (`[::1]`) intact and validating their content.
fn split_host_port(authority: &str) -> Result<(&str, Option<&str>), OAuthError> {
    if let Some(inner) = authority.strip_prefix('[') {
        let Some(close) = inner.find(']') else {
            return Err(invalid_target("resource URI IPv6 literal is not closed"));
        };
        if !is_valid_ip_literal(&inner[..close]) {
            return Err(invalid_target(
                "resource URI bracketed host must be a valid IP literal",
            ));
        }
        let host_end = close + 2; // '[' + literal + ']'
        let host = &authority[..host_end];
        let after_host = &authority[host_end..];
        match after_host.strip_prefix(':') {
            Some(port) => Ok((host, Some(port))),
            None if after_host.is_empty() => Ok((host, None)),
            None => Err(invalid_target("resource URI authority is invalid")),
        }
    } else {
        match authority.rsplit_once(':') {
            Some((host, _)) if host.contains(':') => Err(invalid_target(
                "resource URI IPv6 literal must be enclosed in brackets",
            )),
            Some((host, port)) => Ok((host, Some(port))),
            None => Ok((authority, None)),
        }
    }
}

/// Checks that the content of a bracketed host is an IP literal per
/// RFC 3986 §3.2.2: an IPv6 address or an IPvFuture
/// (`"v" 1*HEXDIG "." 1*(unreserved / sub-delims / ":")`).
///
/// Zone identifiers (RFC 6874, `[fe80::1%25eth0]`) are rejected: link-local
/// addresses are not meaningful as resource indicators.
fn is_valid_ip_literal(literal: &str) -> bool {
    if let Some(rest) = literal.strip_prefix(['v', 'V']) {
        let Some((version, addr)) = rest.split_once('.') else {
            return false;
        };
        !version.is_empty()
            && version.bytes().all(|b| b.is_ascii_hexdigit())
            && !addr.is_empty()
            && addr.bytes().all(|b| {
                b.is_ascii_alphanumeric()
                    || matches!(
                        b,
                        b'-' | b'.'
                            | b'_'
                            | b'~'
                            | b'!'
                            | b'$'
                            | b'&'
                            | b'\''
                            | b'('
                            | b')'
                            | b'*'
                            | b'+'
                            | b','
                            | b';'
                            | b'='
                            | b':'
                    )
            })
    } else {
        literal.parse::<Ipv6Addr>().is_ok()
    }
}

#[inline]
fn invalid_target(description: &str) -> OAuthError {
    OAuthError::new(OAuthErrorCode::InvalidTarget).with_description(description)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_renders_empty_challenge() {
        assert_eq!(BearerChallenge::new().to_string(), "Bearer");
    }

    #[test]
    fn it_renders_error_and_description() {
        let challenge = BearerChallenge::new()
            .with_error(OAuthErrorCode::InvalidToken)
            .with_description("Token has expired");
        assert_eq!(
            challenge.to_string(),
            r#"Bearer error="invalid_token", error_description="Token has expired""#
        );
    }

    #[test]
    fn it_renders_all_parameters_in_stable_order() {
        let challenge = BearerChallenge::new()
            .with_resource_metadata("https://api.example.com/.well-known/oauth-protected-resource")
            .with_scope("read write")
            .with_description("Insufficient privileges")
            .with_error(OAuthErrorCode::InsufficientScope)
            .with_realm("api");
        assert_eq!(
            challenge.to_string(),
            r#"Bearer realm="api", error="insufficient_scope", error_description="Insufficient privileges", scope="read write", resource_metadata="https://api.example.com/.well-known/oauth-protected-resource""#
        );
    }

    #[test]
    fn it_escapes_quotes_and_backslashes() {
        let challenge = BearerChallenge::new().with_description(r#"a "quoted" \ value"#);
        assert_eq!(
            challenge.to_string(),
            r#"Bearer error_description="a \"quoted\" \\ value""#
        );
    }

    #[test]
    fn it_replaces_control_characters_with_spaces() {
        let challenge = BearerChallenge::new().with_description("line\r\nbreak\tand tab");
        assert_eq!(
            challenge.to_string(),
            r#"Bearer error_description="line  break and tab""#
        );
    }

    #[test]
    fn it_renders_custom_error_code() {
        let challenge = BearerChallenge::new().with_error(OAuthErrorCode::from("use_dpop_nonce"));
        assert_eq!(challenge.to_string(), r#"Bearer error="use_dpop_nonce""#);
    }

    #[test]
    fn it_canonicalizes_scheme_and_host_case() {
        assert_eq!(
            canonicalize_resource_uri("HTTPS://API.Example.COM/Path/Sub").unwrap(),
            "https://api.example.com/Path/Sub"
        );
    }

    #[test]
    fn it_strips_default_ports() {
        assert_eq!(
            canonicalize_resource_uri("https://example.com:443/api").unwrap(),
            "https://example.com/api"
        );
        assert_eq!(
            canonicalize_resource_uri("http://example.com:80/api").unwrap(),
            "http://example.com/api"
        );
        assert_eq!(
            canonicalize_resource_uri("wss://example.com:443/socket").unwrap(),
            "wss://example.com/socket"
        );
    }

    #[test]
    fn it_keeps_non_default_ports() {
        assert_eq!(
            canonicalize_resource_uri("https://example.com:8443/api").unwrap(),
            "https://example.com:8443/api"
        );
    }

    #[test]
    fn it_drops_root_path_and_empty_port() {
        assert_eq!(
            canonicalize_resource_uri("https://example.com/").unwrap(),
            "https://example.com"
        );
        assert_eq!(
            canonicalize_resource_uri("https://example.com").unwrap(),
            "https://example.com"
        );
        assert_eq!(
            canonicalize_resource_uri("https://example.com:").unwrap(),
            "https://example.com"
        );
    }

    #[test]
    fn it_preserves_query_and_non_root_trailing_slash() {
        assert_eq!(
            canonicalize_resource_uri("https://example.com/api/?page=1").unwrap(),
            "https://example.com/api/?page=1"
        );
        assert_eq!(
            canonicalize_resource_uri("https://example.com/?q=1").unwrap(),
            "https://example.com/?q=1"
        );
    }

    #[test]
    fn it_canonicalizes_ipv6_literals() {
        assert_eq!(
            canonicalize_resource_uri("https://[2001:DB8::1]:443/api").unwrap(),
            "https://[2001:db8::1]/api"
        );
        assert_eq!(
            canonicalize_resource_uri("https://[::1]:8443").unwrap(),
            "https://[::1]:8443"
        );
    }

    #[test]
    fn it_keeps_ip_vfuture_literals() {
        assert_eq!(
            canonicalize_resource_uri("https://[v1.FE:x]:8443/api").unwrap(),
            "https://[v1.fe:x]:8443/api"
        );
    }

    #[test]
    fn it_keeps_urn_style_uris() {
        assert_eq!(
            canonicalize_resource_uri("URN:example:resource").unwrap(),
            "urn:example:resource"
        );
    }

    #[test]
    fn it_rejects_invalid_resource_uris() {
        let cases = [
            "",
            "not a uri",
            "/relative/path",
            "https://example.com/api#section",
            "https://user@example.com/api",
            "https://example.com:8o80/api",
            "https://",
            "https://[::1/api",
            "https://2001:db8::1/api",
            "1https://example.com",
            "https:api.example.com",
            "https:/api.example.com",
            "WS:example.com/socket",
            "https://[]",
            "https://[not-an-ip]/api",
            "https://[1.2.3.4]",
            "https://[fe80::1%25eth0]/api",
            "https://[v.abc]",
            "https://[v1.]",
        ];
        for uri in cases {
            let err = canonicalize_resource_uri(uri).unwrap_err();
            assert_eq!(err.error, OAuthErrorCode::InvalidTarget, "case: {uri}");
        }
    }
}
