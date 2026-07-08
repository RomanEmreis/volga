//! Shared OAuth utilities
//!
//! * [`BearerChallenge`] — builder and parser for `WWW-Authenticate: Bearer ...`
//!   challenges per [RFC 6750 §3](https://www.rfc-editor.org/rfc/rfc6750#section-3)
//!   and [RFC 9728 §5.1](https://www.rfc-editor.org/rfc/rfc9728#section-5.1)
//! * [`canonicalize_resource_uri`] — resource indicator normalization per
//!   [RFC 8707 §2](https://www.rfc-editor.org/rfc/rfc8707#section-2)

use super::error::{OAuthError, OAuthErrorCode};
use std::fmt::{self, Display, Formatter, Write};
use std::net::Ipv6Addr;
use std::str::FromStr;

/// Builder and parser for a `WWW-Authenticate: Bearer` challenge header value
///
/// Parameters are emitted in a stable order: `realm`, `error`,
/// `error_description`, `scope`, `resource_metadata`. All values are
/// quoted; embedded `"` and `\` are escaped and control characters are
/// replaced with spaces so the result is always a valid header value.
///
/// The inverse operation — extracting a `Bearer` challenge from a received
/// `WWW-Authenticate` header value — is available via
/// [`BearerChallenge::parse`] (also exposed through [`FromStr`]).
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

    /// Returns the `realm` parameter, if set
    #[inline]
    pub fn realm(&self) -> Option<&str> {
        self.realm.as_deref()
    }

    /// Returns the `error` parameter, if set
    #[inline]
    pub fn error(&self) -> Option<&OAuthErrorCode> {
        self.error.as_ref()
    }

    /// Returns the `error_description` parameter, if set
    #[inline]
    pub fn description(&self) -> Option<&str> {
        self.error_description.as_deref()
    }

    /// Returns the `scope` parameter, if set
    #[inline]
    pub fn scope(&self) -> Option<&str> {
        self.scope.as_deref()
    }

    /// Returns the `resource_metadata` parameter, if set
    #[inline]
    pub fn resource_metadata(&self) -> Option<&str> {
        self.resource_metadata.as_deref()
    }

    /// Parses a `WWW-Authenticate` header value and extracts the `Bearer`
    /// challenge from it
    ///
    /// The header may carry several comma-separated challenges
    /// ([RFC 9110 §11.6.1](https://www.rfc-editor.org/rfc/rfc9110#section-11.6.1));
    /// the first `Bearer` challenge is used and the auth scheme is matched
    /// case-insensitively. Parameter names are matched case-insensitively
    /// as well, values may be given as tokens or quoted strings
    /// (quoted-pair escapes are decoded), and unrecognized parameters are
    /// ignored for forward compatibility.
    ///
    /// Returns an [`OAuthError`] with code `invalid_request` when the value
    /// contains no `Bearer` challenge or is malformed: an invalid scheme or
    /// parameter name, a value that is neither a token nor a quoted string,
    /// an unterminated quoted string, a control character, a duplicated
    /// recognized parameter, or a `token68` payload, which the Bearer scheme
    /// does not use ([RFC 6750 §3](https://www.rfc-editor.org/rfc/rfc6750#section-3)).
    ///
    /// # Example
    /// ```
    /// use volga::auth::oauth::{BearerChallenge, OAuthErrorCode};
    ///
    /// let challenge = BearerChallenge::parse(
    ///     r#"Basic realm="legacy", Bearer error="invalid_token", error_description="Token has expired""#,
    /// ).unwrap();
    ///
    /// assert_eq!(challenge.error(), Some(&OAuthErrorCode::InvalidToken));
    /// assert_eq!(challenge.description(), Some("Token has expired"));
    /// ```
    pub fn parse(header: &str) -> Result<Self, OAuthError> {
        let mut bearer: Option<Self> = None;
        let mut seen_scheme = false;
        for element in split_list_elements(header)? {
            // Empty list elements (`Bearer, , Basic`) are legal per RFC 9110 §5.6.1
            if element.is_empty() {
                continue;
            }
            match classify_element(element) {
                Element::Scheme { scheme, param } => {
                    // The Bearer challenge ends where the next challenge begins
                    if bearer.is_some() {
                        break;
                    }
                    if !scheme.bytes().all(is_tchar) {
                        return Err(invalid_challenge("auth scheme is not a valid token"));
                    }
                    seen_scheme = true;
                    if scheme.eq_ignore_ascii_case("Bearer") {
                        let mut challenge = Self::new();
                        if let Some(param) = param {
                            let (name, value) = parse_auth_param(param)?;
                            challenge.set_param(&name, value)?;
                        }
                        bearer = Some(challenge);
                    }
                }
                Element::Param => {
                    if let Some(challenge) = &mut bearer {
                        let (name, value) = parse_auth_param(element)?;
                        challenge.set_param(&name, value)?;
                    } else if !seen_scheme {
                        return Err(invalid_challenge(
                            "auth parameter appears before any challenge scheme",
                        ));
                    }
                    // Parameters of other schemes are skipped
                }
            }
        }
        bearer.ok_or_else(|| invalid_challenge("no Bearer challenge found"))
    }

    /// Stores a parsed auth parameter; `name` must already be lowercased.
    /// Unknown parameters are ignored for forward compatibility, duplicates
    /// of recognized ones are rejected (RFC 7235 §2.1).
    fn set_param(&mut self, name: &str, value: String) -> Result<(), OAuthError> {
        let slot = match name {
            "realm" => &mut self.realm,
            "error_description" => &mut self.error_description,
            "scope" => &mut self.scope,
            "resource_metadata" => &mut self.resource_metadata,
            "error" => {
                return if self.error.replace(OAuthErrorCode::from(value)).is_some() {
                    Err(invalid_challenge("duplicate parameter in Bearer challenge"))
                } else {
                    Ok(())
                };
            }
            _ => return Ok(()),
        };
        if slot.replace(value).is_some() {
            return Err(invalid_challenge("duplicate parameter in Bearer challenge"));
        }
        Ok(())
    }
}

impl FromStr for BearerChallenge {
    type Err = OAuthError;

    #[inline]
    fn from_str(header: &str) -> Result<Self, Self::Err> {
        Self::parse(header)
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

/// A single comma-separated element of a `WWW-Authenticate` header value:
/// either the start of a challenge (a bare scheme, optionally followed by
/// its first space-separated item) or an auth parameter belonging to the
/// current challenge.
enum Element<'a> {
    Scheme {
        scheme: &'a str,
        param: Option<&'a str>,
    },
    Param,
}

/// Classifies a non-empty, trimmed list element. An element starts a new
/// challenge when it is a bare token or a token separated from the rest by
/// whitespace; `name=value` pairs (with optional bad whitespace around `=`,
/// RFC 9110 §5.6.3) are parameters of the current challenge.
fn classify_element(element: &str) -> Element<'_> {
    match element.split_once([' ', '\t']) {
        None if !element.contains('=') => Element::Scheme {
            scheme: element,
            param: None,
        },
        Some((scheme, rest)) if !scheme.contains('=') && !rest.trim_start().starts_with('=') => {
            Element::Scheme {
                scheme,
                param: Some(rest.trim_start()),
            }
        }
        _ => Element::Param,
    }
}

/// Splits a header value at top-level commas, leaving commas inside quoted
/// strings (including escaped quotes) intact. Elements are trimmed but may
/// be empty — the `#challenge` list grammar allows empty elements.
fn split_list_elements(header: &str) -> Result<Vec<&str>, OAuthError> {
    let bytes = header.as_bytes();
    let mut elements = Vec::new();
    let (mut start, mut index, mut in_quotes) = (0, 0, false);
    while index < bytes.len() {
        match bytes[index] {
            b'"' => in_quotes = !in_quotes,
            // Skip the escaped byte; multi-byte characters are left alone
            // since only ASCII `"` and `,` are meaningful here
            b'\\' if in_quotes => index += 1,
            b',' if !in_quotes => {
                elements.push(header[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
        index += 1;
    }
    if in_quotes {
        return Err(invalid_challenge("quoted string is not terminated"));
    }
    elements.push(header[start..].trim());
    Ok(elements)
}

/// Parses a single `name=value` auth parameter (RFC 9110 §11.2): the name
/// is lowercased, the value is either a token or a quoted string with
/// quoted-pair escapes decoded. Bad whitespace around `=` is tolerated.
fn parse_auth_param(element: &str) -> Result<(String, String), OAuthError> {
    let Some((name, value)) = element.split_once('=') else {
        return Err(invalid_challenge("auth parameter is missing '='"));
    };
    let name = name.trim_end();
    if name.is_empty() || !name.bytes().all(is_tchar) {
        return Err(invalid_challenge(
            "auth parameter name is not a valid token",
        ));
    }
    let value = value.trim_start();
    let value = if let Some(quoted) = value.strip_prefix('"') {
        unquote(quoted)?
    } else if !value.is_empty() && value.bytes().all(is_tchar) {
        value.to_owned()
    } else {
        return Err(invalid_challenge(
            "auth parameter value must be a token or a quoted string",
        ));
    };
    Ok((name.to_ascii_lowercase(), value))
}

/// Decodes the remainder of a quoted string (the opening `"` already
/// stripped): `\x` escapes are unquoted, control characters other than
/// HTAB are rejected, and nothing may follow the closing quote.
fn unquote(quoted: &str) -> Result<String, OAuthError> {
    let mut value = String::with_capacity(quoted.len());
    let mut symbols = quoted.chars();
    while let Some(symbol) = symbols.next() {
        match symbol {
            '"' => {
                return if symbols.as_str().trim().is_empty() {
                    Ok(value)
                } else {
                    Err(invalid_challenge(
                        "unexpected content after a quoted string",
                    ))
                };
            }
            '\\' => match symbols.next() {
                Some(escaped) if escaped == '\t' || !escaped.is_control() => value.push(escaped),
                _ => return Err(invalid_challenge("invalid escape in a quoted string")),
            },
            symbol if symbol.is_control() && symbol != '\t' => {
                return Err(invalid_challenge("control character in a quoted string"));
            }
            symbol => value.push(symbol),
        }
    }
    Err(invalid_challenge("quoted string is not terminated"))
}

/// Checks the RFC 9110 §5.6.2 `tchar` grammar (token characters)
fn is_tchar(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#'
                | b'$'
                | b'%'
                | b'&'
                | b'\''
                | b'*'
                | b'+'
                | b'-'
                | b'.'
                | b'^'
                | b'_'
                | b'`'
                | b'|'
                | b'~'
        )
}

#[inline]
fn invalid_challenge(description: &str) -> OAuthError {
    OAuthError::new(OAuthErrorCode::InvalidRequest).with_description(description)
}

/// Canonicalizes an OAuth 2.0 resource indicator (RFC 8707) so that
/// equivalent URIs compare equal as strings (e.g. for `aud` matching).
///
/// Normalization applied:
/// * the scheme and host are lowercased;
/// * default ports are removed (`http`/`ws`: 80, `https`/`wss`: 443);
/// * for web schemes (`http`, `https`, `ws`, `wss`) a lone root path is
///   dropped, both bare (`https://example.com/` → `https://example.com`)
///   and before a query (`…/?q=1` → `…?q=1`); for other schemes the path is
///   preserved verbatim, as the empty-path/`/` equivalence is
///   scheme-specific (RFC 3986 §6.2.3). Non-root paths and query strings
///   are always preserved.
///
/// Returns an [`OAuthError`] with code `invalid_target` when the URI is not
/// an absolute URI, contains a fragment, userinfo, whitespace, control or
/// non-ASCII characters, uses a web scheme (`http`, `https`, `ws`, `wss`)
/// without an authority (`https:api.example.com`), has a bracketed host that
/// is not a valid IPv6/IPvFuture literal, an unbracketed host with
/// characters outside the RFC 3986 `reg-name` grammar, or a path/query with
/// characters outside the `pchar` / `query` grammar (including incomplete
/// percent-escapes). Percent-encoding and dot-segment normalization are
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
    let is_web_scheme = matches!(scheme.as_str(), "http" | "https" | "ws" | "wss");

    let Some(after_scheme) = rest.strip_prefix("//") else {
        // Web schemes always carry an authority: `https:api.example.com`
        // or `https:/api` is a mistyped resource, not a URN-style URI
        if is_web_scheme {
            return Err(invalid_target("resource URI must have an authority"));
        }
        // No authority component (e.g. `urn:example:resource`) — the
        // scheme-specific part is pchar-based too (`hier-part [ "?" query ]`,
        // RFC 3986 §3.3), and only the scheme is subject to normalization
        if !is_valid_uri_component(rest, b":@/?") {
            return Err(invalid_target("resource URI contains invalid characters"));
        }
        return Ok(format!("{scheme}:{rest}"));
    };

    let authority_end = after_scheme.find(['/', '?']).unwrap_or(after_scheme.len());
    let (authority, path_and_query) = after_scheme.split_at(authority_end);
    if authority.contains('@') {
        return Err(invalid_target("resource URI must not contain userinfo"));
    }
    // `pchar` plus the `/` and `?` delimiters (RFC 3986 §3.3–3.4); `#` was
    // already rejected above, so everything after the first `?` is the query
    if !is_valid_uri_component(path_and_query, b":@/?") {
        return Err(invalid_target(
            "resource URI path or query contains invalid characters",
        ));
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
    // Empty-path/`/` equivalence is scheme-based normalization
    // (RFC 3986 §6.2.3) — only apply it to schemes we know. The lone root
    // slash is dropped both bare (`/`) and before a query (`/?q=1`)
    if is_web_scheme && (path_and_query == "/" || path_and_query.starts_with("/?")) {
        result.push_str(&path_and_query[1..]);
    } else {
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
        let (host, port) = match authority.rsplit_once(':') {
            Some((host, _)) if host.contains(':') => {
                return Err(invalid_target(
                    "resource URI IPv6 literal must be enclosed in brackets",
                ));
            }
            Some((host, port)) => (host, Some(port)),
            None => (authority, None),
        };
        // `reg-name = *( unreserved / pct-encoded / sub-delims )` (§3.2.2)
        if !is_valid_uri_component(host, b"") {
            return Err(invalid_target(
                "resource URI host contains invalid characters",
            ));
        }
        Ok((host, port))
    }
}

/// Checks that a string consists of RFC 3986 `unreserved` / `sub-delims`
/// characters, complete percent-escapes (`%` followed by two hex digits;
/// their decoding is not performed) and the given extra delimiter bytes.
///
/// With no extras this is exactly the `reg-name` grammar (§3.2.2); with
/// `b":@/?"` it covers a path with an optional query (§3.3–3.4).
fn is_valid_uri_component(component: &str, extra: &[u8]) -> bool {
    let mut bytes = component.bytes();
    while let Some(byte) = bytes.next() {
        match byte {
            b'%' => {
                let valid_escape = matches!(
                    (bytes.next(), bytes.next()),
                    (Some(hi), Some(lo)) if hi.is_ascii_hexdigit() && lo.is_ascii_hexdigit()
                );
                if !valid_escape {
                    return false;
                }
            }
            byte if byte.is_ascii_alphanumeric() => {}
            b'-' | b'.' | b'_' | b'~' | b'!' | b'$' | b'&' | b'\'' | b'(' | b')' | b'*' | b'+'
            | b',' | b';' | b'=' => {}
            byte if extra.contains(&byte) => {}
            _ => return false,
        }
    }
    true
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
    fn it_parses_full_challenge() {
        let challenge = BearerChallenge::parse(
            r#"Bearer realm="api", error="insufficient_scope", error_description="Insufficient privileges", scope="read write", resource_metadata="https://api.example.com/.well-known/oauth-protected-resource""#,
        )
        .unwrap();
        assert_eq!(challenge.realm(), Some("api"));
        assert_eq!(challenge.error(), Some(&OAuthErrorCode::InsufficientScope));
        assert_eq!(challenge.description(), Some("Insufficient privileges"));
        assert_eq!(challenge.scope(), Some("read write"));
        assert_eq!(
            challenge.resource_metadata(),
            Some("https://api.example.com/.well-known/oauth-protected-resource")
        );
    }

    #[test]
    fn it_roundtrips_built_challenge() {
        let original = BearerChallenge::new()
            .with_realm("api")
            .with_error(OAuthErrorCode::InvalidToken)
            .with_description(r#"a "quoted" \ value"#)
            .with_scope("read write")
            .with_resource_metadata("https://api.example.com/.well-known/oauth-protected-resource");
        let parsed = BearerChallenge::parse(&original.to_string()).unwrap();
        assert_eq!(parsed, original);
    }

    #[test]
    fn it_parses_empty_bearer_challenge() {
        assert_eq!(
            BearerChallenge::parse("Bearer").unwrap(),
            BearerChallenge::new()
        );
        assert_eq!(
            BearerChallenge::parse("  bearer  ").unwrap(),
            BearerChallenge::new()
        );
    }

    #[test]
    fn it_parses_case_insensitively_and_accepts_token_values() {
        let challenge = BearerChallenge::parse("BEARER ERROR=invalid_token, Realm=api").unwrap();
        assert_eq!(challenge.error(), Some(&OAuthErrorCode::InvalidToken));
        assert_eq!(challenge.realm(), Some("api"));
    }

    #[test]
    fn it_tolerates_bad_whitespace_around_equals() {
        let challenge = BearerChallenge::parse(r#"Bearer realm = "api", scope= read"#).unwrap();
        assert_eq!(challenge.realm(), Some("api"));
        assert_eq!(challenge.scope(), Some("read"));
    }

    #[test]
    fn it_picks_bearer_among_multiple_challenges() {
        let challenge = BearerChallenge::parse(
            r#"Negotiate Zm9vYmFyCg==, Newauth title="Login, please", Bearer realm="api", Basic realm="other""#,
        )
        .unwrap();
        assert_eq!(challenge.realm(), Some("api"));
        assert_eq!(challenge.error(), None);
    }

    #[test]
    fn it_ignores_unknown_parameters() {
        let challenge =
            BearerChallenge::parse(r#"Bearer nonce="abc", error="use_dpop_nonce""#).unwrap();
        assert_eq!(
            challenge.error(),
            Some(&OAuthErrorCode::from("use_dpop_nonce"))
        );
        assert_eq!(challenge.realm(), None);
    }

    #[test]
    fn it_skips_empty_list_elements() {
        let challenge = BearerChallenge::parse(r#", Bearer realm="api", ,"#).unwrap();
        assert_eq!(challenge.realm(), Some("api"));
    }

    #[test]
    fn it_parses_via_from_str() {
        let challenge: BearerChallenge = r#"Bearer scope="read""#.parse().unwrap();
        assert_eq!(challenge.scope(), Some("read"));
    }

    #[test]
    fn it_rejects_malformed_challenges() {
        let cases = [
            "",
            "   ",
            r#"Basic realm="api""#,           // no Bearer challenge
            r#"realm="api", Bearer"#,         // parameter before any scheme
            r#"Bearer realm="api"#,           // unterminated quoted string
            "Bearer realm=",                  // empty value
            r#"Bearer realm="a" junk"#,       // content after a quoted string
            "Bearer foo bar",                 // missing '='
            "Bearer Zm9vYmFyCg==",            // token68 payload
            "Bearer =x",                      // parameter with an empty name
            r#"Bearer re alm="x""#,           // invalid parameter name
            r#"Bearer realm="a", realm="b""#, // duplicate parameter
            r#"Bearer error="a", error="b""#, // duplicate error
            "Bearer realm=\"a\u{1}b\"",       // control character in a value
            r#"Bearer realm=\"#,              // bare backslash value
            r#"foo/bar, Bearer realm="x""#,   // malformed auth scheme
        ];
        for header in cases {
            let err = BearerChallenge::parse(header).unwrap_err();
            assert_eq!(err.error, OAuthErrorCode::InvalidRequest, "case: {header}");
        }
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
    }

    #[test]
    fn it_normalizes_root_path_before_query() {
        assert_eq!(
            canonicalize_resource_uri("https://example.com/?q=1").unwrap(),
            "https://example.com?q=1"
        );
        assert_eq!(
            canonicalize_resource_uri("https://example.com?q=1").unwrap(),
            "https://example.com?q=1"
        );
        // Not scheme-equivalent for custom schemes — both forms are kept
        assert_eq!(
            canonicalize_resource_uri("foo://api/?q=1").unwrap(),
            "foo://api/?q=1"
        );
        assert_eq!(
            canonicalize_resource_uri("foo://api?q=1").unwrap(),
            "foo://api?q=1"
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
    fn it_keeps_pct_encoded_and_sub_delim_hosts() {
        assert_eq!(
            canonicalize_resource_uri("https://ex%41mple.com/api").unwrap(),
            "https://ex%41mple.com/api"
        );
        assert_eq!(
            canonicalize_resource_uri("https://api.ex-ample_1.com").unwrap(),
            "https://api.ex-ample_1.com"
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
    fn it_keeps_valid_pchar_and_query_characters() {
        assert_eq!(
            canonicalize_resource_uri("https://api.example.com/a%20b/v1:x@y?q=?&r=/").unwrap(),
            "https://api.example.com/a%20b/v1:x@y?q=?&r=/"
        );
    }

    #[test]
    fn it_preserves_root_path_and_port_for_custom_schemes() {
        assert_eq!(
            canonicalize_resource_uri("FOO://API.Example.com/").unwrap(),
            "foo://api.example.com/"
        );
        assert_eq!(
            canonicalize_resource_uri("foo://api.example.com").unwrap(),
            "foo://api.example.com"
        );
        assert_eq!(
            canonicalize_resource_uri("foo://api.example.com:80/x").unwrap(),
            "foo://api.example.com:80/x"
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
            "https://exa[mple.com",
            "https://exa\\mple.com/api",
            "https://example.com|evil",
            "https://ex%2Gmple.com",
            "https://example.com%2",
            "https://api.example.com/%",
            "https://api.example.com/a%2",
            "https://api.example.com/|evil",
            "https://api.example.com/x?a=^b",
            "https://api.example.com/a\\b",
            "urn:example:res|ource",
            "urn:example:a%2",
        ];
        for uri in cases {
            let err = canonicalize_resource_uri(uri).unwrap_err();
            assert_eq!(err.error, OAuthErrorCode::InvalidTarget, "case: {uri}");
        }
    }
}
