//! Tools and utils for Authorization & Authentication

#[cfg(feature = "jwt-auth")]
use {
    crate::headers::{CACHE_CONTROL, HeaderValue, WWW_AUTHENTICATE, cache_control::NO_STORE},
    crate::middleware::{HttpContext, Middleware, NextFn},
    crate::{
        App, HttpResult,
        error::Error,
        http::StatusCode,
        routing::{Route, RouteGroup},
    },
    std::{future::Future, sync::Arc},
};

#[cfg(feature = "jwt-auth")]
pub use {
    algorithm::Algorithm,
    authenticated::Authenticated,
    authorizer::{Authorizer, permission, permissions, predicate, role, roles},
    bearer::{Bearer, BearerAuthConfig, BearerTokenService},
    claims::AuthClaims,
    decoding_key::DecodingKey,
    encoding_key::EncodingKey,
};

#[cfg(feature = "jwt-auth")]
use jsonwebtoken::errors::{Error as JwtError, ErrorKind};

#[cfg(feature = "jwt-derive")]
pub use volga_macros::Claims;

#[cfg(feature = "basic-auth")]
pub use basic::Basic;

#[cfg(feature = "jwt-auth")]
pub mod algorithm;
#[cfg(feature = "jwt-auth")]
pub mod authenticated;
#[cfg(feature = "jwt-auth")]
pub mod authorizer;
#[cfg(feature = "basic-auth")]
pub mod basic;
#[cfg(feature = "jwt-auth")]
pub mod bearer;
#[cfg(feature = "jwt-auth")]
pub mod claims;
#[cfg(feature = "jwt-auth")]
pub mod decoding_key;
#[cfg(feature = "jwt-auth")]
pub mod encoding_key;
#[cfg(feature = "jwt-auth")]
pub(crate) mod key;
#[cfg(feature = "oauth")]
pub mod oauth;
#[cfg(feature = "oauth-client")]
pub mod oauth_client;

#[cfg(feature = "oauth-client")]
pub use oauth_client::{DEFAULT_MAX_KEY_AGE, DEFAULT_REFRESH_COOLDOWN, OAuthConfig};

#[cfg(feature = "jwt-auth")]
impl Error {
    /// Converts [`jsonwebtoken::errors::Error`] into [`volga::Error`]
    #[inline]
    pub(crate) fn from_jwt_error(err: JwtError) -> Self {
        let kind = err.kind();
        let status_code = map_jwt_error_to_status(kind);
        Error::from_parts(status_code, None, err)
    }
}

#[cfg(feature = "jwt-auth")]
fn map_jwt_error_to_status(err: &ErrorKind) -> StatusCode {
    use ErrorKind::*;
    match err {
        ExpiredSignature
        | InvalidToken
        | InvalidIssuer
        | InvalidAudience
        | InvalidSubject
        | InvalidSignature
        | MissingRequiredClaim(_)
        | ImmatureSignature
        | InvalidAlgorithmName
        | InvalidAlgorithm
        // A credential that does not decode is a malformed access token, which RFC 6750
        // Section 3.1 answers `401` with `invalid_token`. Its `400` and `invalid_request`
        // are for a request that is wrong about how it carries a token rather than about
        // the token - and a client told `400` reads its own request as the thing to fix,
        // where what it is holding is a credential to replace
        | Base64(_)
        | Json(_)
        | Utf8(_) => StatusCode::UNAUTHORIZED,
        // The key this server verifies with is its own configuration. The caller has
        // nothing to fix and nothing to learn from being told, so this is a validation that
        // could not be completed rather than a credential that did not hold up
        InvalidKeyFormat => StatusCode::INTERNAL_SERVER_ERROR,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

/// The `error` code and the description a token that failed to decode is challenged with
#[cfg(feature = "jwt-auth")]
fn describe_jwt_error(err: &ErrorKind) -> (oauth::OAuthErrorCode, &'static str) {
    use crate::auth::oauth::OAuthErrorCode::{InvalidToken as InvalidTokenCode, ServerError};
    use ErrorKind::*;

    match err {
        ExpiredSignature => (InvalidTokenCode, "Token has expired"),
        InvalidSignature => (InvalidTokenCode, "Invalid signature"),
        InvalidToken => (InvalidTokenCode, "Token is malformed or invalid"),
        ImmatureSignature => (InvalidTokenCode, "Token is not valid yet (nbf)"),
        MissingRequiredClaim(_) => (InvalidTokenCode, "Missing required claim"),
        InvalidIssuer => (InvalidTokenCode, "Invalid issuer (iss)"),
        InvalidAudience => (InvalidTokenCode, "Invalid audience (aud)"),
        InvalidSubject => (InvalidTokenCode, "Invalid subject (sub)"),
        InvalidAlgorithm | InvalidAlgorithmName => (InvalidTokenCode, "Invalid algorithm"),
        Base64(_) => (InvalidTokenCode, "Token is not properly base64-encoded"),
        Json(_) => (InvalidTokenCode, "Token payload is not valid JSON"),
        Utf8(_) => (InvalidTokenCode, "Token contains invalid UTF-8 characters"),
        InvalidKeyFormat => (ServerError, "Invalid key format"),
        _ => (ServerError, "Internal token processing error"),
    }
}

#[cfg(feature = "jwt-auth")]
fn build_www_authenticate(err: &ErrorKind, resource_metadata_url: Option<&str>) -> String {
    let (code, description) = describe_jwt_error(err);

    let mut challenge = oauth::BearerChallenge::new()
        .with_error(code)
        .with_description(description);

    if let Some(url) = resource_metadata_url {
        challenge = challenge.with_resource_metadata(url);
    }

    challenge.to_string()
}

#[cfg(feature = "jwt-auth")]
impl App {
    /// Configures a web server with a Bearer Token Authentication & Authorization configuration
    ///
    /// Default: `None`
    ///
    /// # Example
    /// ```no_run
    /// use volga::App;
    ///
    /// let app = App::new()
    ///     .with_bearer_auth(|auth| auth);
    /// ```
    pub fn with_bearer_auth<F>(self, config: F) -> Self
    where
        F: FnOnce(BearerAuthConfig) -> BearerAuthConfig,
    {
        self.set_bearer_auth(config(Default::default()))
    }

    /// Configures a web server with the specified Bearer Token Authentication & Authorization
    /// configuration
    ///
    /// The value form of [`with_bearer_auth`](App::with_bearer_auth), for a configuration
    /// built elsewhere.
    ///
    /// Default: `None`
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, auth::BearerAuthConfig};
    ///
    /// let auth = BearerAuthConfig::default();
    /// let app = App::new().set_bearer_auth(auth);
    /// ```
    pub fn set_bearer_auth(mut self, config: BearerAuthConfig) -> Self {
        self.auth_config = Some(config);
        self
    }

    /// Describes the OAuth 2.1/OIDC issuer whose keys validate bearer
    /// tokens - activate it explicitly with [`use_oauth`](App::use_oauth)
    ///
    /// The issuer metadata and its JSON Web Key Set are fetched lazily on
    /// the first request and refreshed on key rotation; see
    /// [`OAuthConfig`] for the knobs. Token checks other than the key and
    /// `iss` (audience, expiry, ...) stay on
    /// [`with_bearer_auth`](App::with_bearer_auth).
    ///
    /// With the `config` feature the same knobs can come from the
    /// `[oauth.client]` section of the configuration file instead -
    /// [`use_oauth`](App::use_oauth) remains a code-only call either way.
    ///
    /// # Example
    /// ```no_run
    /// use volga::App;
    ///
    /// let mut app = App::new()
    ///     .with_oauth(|oauth| oauth.with_issuer("https://auth.example.com"));
    ///
    /// app.use_oauth();
    /// ```
    #[cfg(feature = "oauth-client")]
    pub fn with_oauth<F>(self, config: F) -> Self
    where
        F: FnOnce(OAuthConfig) -> OAuthConfig,
    {
        self.set_oauth(config(Default::default()))
    }

    /// Describes the OAuth 2.1/OIDC issuer whose keys validate bearer
    /// tokens - activate it explicitly with [`use_oauth`](App::use_oauth)
    ///
    /// The value form of [`with_oauth`](App::with_oauth), for a configuration
    /// built elsewhere.
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, auth::OAuthConfig};
    ///
    /// let oauth = OAuthConfig::default().with_issuer("https://auth.example.com");
    /// let mut app = App::new().set_oauth(oauth);
    ///
    /// app.use_oauth();
    /// ```
    #[cfg(feature = "oauth-client")]
    pub fn set_oauth(mut self, config: OAuthConfig) -> Self {
        self.oauth_client_config = Some(config);
        self
    }

    /// Explicitly enables validation of bearer tokens against the OAuth
    /// issuer configured with [`with_oauth`](App::with_oauth) or
    /// [`set_oauth`](App::set_oauth)
    ///
    /// # Panics
    /// Panics when no OAuth configuration is set (via
    /// [`with_oauth`](App::with_oauth), [`set_oauth`](App::set_oauth) or the
    /// `[oauth.client]` config file section), or when it names no issuer.
    #[cfg(feature = "oauth-client")]
    pub fn use_oauth(&mut self) -> &mut Self {
        match &self.oauth_client_config {
            Some(config) if config.issuer.is_some() => self.oauth_client_enabled = true,
            Some(_) => panic!(
                "OAuth issuer is not configured; call `with_oauth(|oauth| oauth.with_issuer(..))` \
                or `set_oauth(..)` with an issuer first"
            ),
            None => {
                panic!("OAuth is not configured; call `with_oauth(..)` or `set_oauth(..)` first")
            }
        }
        self
    }

    /// Adds authorization middleware for all routes
    ///
    /// A request it refuses is an error, answered by the error handler: `401` without a
    /// bearer token or with one that does not hold up, `400` for an `Authorization` header
    /// that is not a bearer credential or for plain HTTP where HTTPS is required, `403` for
    /// a token without the authority asked for and `503` when the token cannot be validated
    /// right now. Its `WWW-Authenticate` challenge and `Cache-Control: no-store` are added to
    /// whatever the error handler answers with, so `map_problem` renders it as problem
    /// details with the challenge in place - see [`Error::with_header`].
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, auth::{AuthClaims, roles}};
    /// use serde::Deserialize;
    ///
    /// #[derive(Clone, Deserialize)]
    /// struct MyClaims {
    ///     role: String
    /// }
    ///
    /// impl AuthClaims for MyClaims {
    ///     fn role(&self) -> Option<&str> {
    ///         Some(self.role.as_str())
    ///     }
    /// }
    ///
    /// let mut app = App::new()
    ///     .with_bearer_auth(|auth| auth);
    ///
    /// app.authorize::<MyClaims>(roles(["admin", "user"]));
    ///
    /// app.map_get("/hello", || async { "Hello, World!" });
    /// ```
    pub fn authorize<C: AuthClaims + Send + Sync + 'static>(
        &mut self,
        authorizer: Authorizer<C>,
    ) -> &mut Self {
        self.ensure_bearer_auth_configured();
        self.attach(Authorize::new(authorizer))
    }

    fn ensure_bearer_auth_configured(&self) {
        // issuer-based validation resolves keys at runtime - no static
        // decoding key required (activation order with `use_oauth` is
        // checked at startup)
        #[cfg(feature = "oauth-client")]
        if self
            .oauth_client_config
            .as_ref()
            .is_some_and(|config| config.issuer.is_some())
        {
            return;
        }

        let config = match &self.auth_config {
            Some(config) => config,
            _ => panic!("Bearer Auth is not configured"),
        };

        config
            .decoding_key()
            .expect("Bearer Auth security key is not configured");
    }
}

#[cfg(feature = "jwt-auth")]
impl<'a> Route<'a> {
    /// Adds authorization middleware for this route
    ///
    /// A request it refuses is an error, answered by the error handler: `401` without a
    /// bearer token or with one that does not hold up, `400` for an `Authorization` header
    /// that is not a bearer credential or for plain HTTP where HTTPS is required, `403` for
    /// a token without the authority asked for and `503` when the token cannot be validated
    /// right now. Its `WWW-Authenticate` challenge and `Cache-Control: no-store` are added to
    /// whatever the error handler answers with, so `map_problem` renders it as problem
    /// details with the challenge in place - see [`Error::with_header`].
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, auth::{AuthClaims, roles}};
    /// use serde::Deserialize;
    ///
    /// #[derive(Clone, Deserialize)]
    /// struct MyClaims {
    ///     role: String
    /// }
    ///
    /// impl AuthClaims for MyClaims {
    ///     fn role(&self) -> Option<&str> {
    ///         Some(self.role.as_str())
    ///     }
    /// }
    ///
    /// let mut app = App::new()
    ///     .with_bearer_auth(|auth| auth);
    ///
    /// app.map_get("/hello", || async { "Hello, World!" })
    ///     .authorize::<MyClaims>(roles(["admin", "user"]));
    /// ```
    pub fn authorize<C: AuthClaims + Send + Sync + 'static>(
        self,
        authorizer: Authorizer<C>,
    ) -> Self {
        self.ensure_bearer_auth_configured();
        self.attach(Authorize::new(authorizer))
    }
}

#[cfg(feature = "jwt-auth")]
impl<'a> RouteGroup<'a> {
    /// Adds authorization middleware for this group of routes
    ///
    /// A request it refuses is an error, answered by the error handler: `401` without a
    /// bearer token or with one that does not hold up, `400` for an `Authorization` header
    /// that is not a bearer credential or for plain HTTP where HTTPS is required, `403` for
    /// a token without the authority asked for and `503` when the token cannot be validated
    /// right now. Its `WWW-Authenticate` challenge and `Cache-Control: no-store` are added to
    /// whatever the error handler answers with, so `map_problem` renders it as problem
    /// details with the challenge in place - see [`Error::with_header`].
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, auth::{AuthClaims, roles}};
    /// use serde::Deserialize;
    ///
    /// #[derive(Clone, Deserialize)]
    /// struct MyClaims {
    ///     role: String
    /// }
    ///
    /// impl AuthClaims for MyClaims {
    ///     fn role(&self) -> Option<&str> {
    ///         Some(self.role.as_str())
    ///     }
    /// }
    ///
    /// let mut app = App::new()
    ///     .with_bearer_auth(|auth| auth);
    ///
    /// app.group("/api", |api| {
    ///     api.authorize::<MyClaims>(roles(["admin", "user"]));
    ///     api.map_get("/hello", || async { "Hello, World!" });
    /// });
    /// ```
    pub fn authorize<C: AuthClaims + Send + Sync + 'static>(
        &mut self,
        authorizer: Authorizer<C>,
    ) -> &mut Self {
        self.app.ensure_bearer_auth_configured();
        self.attach(Authorize::new(authorizer))
    }
}

#[cfg(feature = "jwt-auth")]
struct Authorize<C>
where
    C: AuthClaims + Send + Sync + 'static,
{
    authorizer: Arc<Authorizer<C>>,
}

#[cfg(feature = "jwt-auth")]
impl<C> Authorize<C>
where
    C: AuthClaims + Send + Sync + 'static,
{
    fn new(a: Authorizer<C>) -> Self {
        Self {
            authorizer: Arc::new(a),
        }
    }
}

#[cfg(feature = "jwt-auth")]
impl<C> Middleware for Authorize<C>
where
    C: AuthClaims + Send + Sync + 'static,
{
    #[inline]
    fn call(
        &self,
        ctx: HttpContext,
        next: NextFn,
    ) -> impl Future<Output = HttpResult> + Send + 'static {
        authorize_impl(Arc::clone(&self.authorizer), ctx, next)
    }
}

#[cfg(feature = "jwt-auth")]
async fn authorize_impl<C>(
    authorizer: Arc<Authorizer<C>>,
    mut ctx: HttpContext,
    next: NextFn,
) -> HttpResult
where
    C: AuthClaims + Send + Sync + 'static,
{
    let bts: BearerTokenService = ctx.extract()?;
    if should_reject_for_https(&ctx, &bts)? {
        return Err(refusal(
            StatusCode::BAD_REQUEST,
            "HTTPS required",
            Some(r#"Bearer error="invalid_request", error_description="HTTPS required""#.into()),
        ));
    }

    let metadata_url = bts.resource_metadata_url.as_deref();
    let bearer = match resolve_bearer(&ctx) {
        Ok(bearer) => bearer,
        Err(_) => {
            let mut challenge = oauth::BearerChallenge::new();
            if let Some(url) = metadata_url {
                challenge = challenge.with_resource_metadata(url);
            }
            return Err(
                if ctx
                    .request()
                    .headers()
                    .contains_key(crate::headers::AUTHORIZATION)
                {
                    // RFC 6750 Section 3.1: credentials were presented but are not
                    // a well-formed Bearer value (wrong scheme, empty token)
                    // - the client should fix the header, not start a flow
                    const MESSAGE: &str = "Authorization header is not a valid Bearer credential";
                    let challenge = challenge
                        .with_error(oauth::OAuthErrorCode::InvalidRequest)
                        .with_description(MESSAGE);

                    refusal(
                        StatusCode::BAD_REQUEST,
                        MESSAGE,
                        Some(challenge.to_string()),
                    )
                } else {
                    // RFC 6750 Section 3: no credentials were presented - challenge
                    // with a bare scheme (no error code) so clients can
                    // discover the resource metadata and start an
                    // authorization flow
                    refusal(
                        StatusCode::UNAUTHORIZED,
                        "Bearer token is missing",
                        Some(challenge.to_string()),
                    )
                },
            );
        }
    };

    #[cfg(feature = "oauth-client")]
    let decoded = bts.decode_async(bearer.clone()).await;
    #[cfg(not(feature = "oauth-client"))]
    let decoded = bts.decode(bearer.clone());

    let claims = match decoded {
        Ok(claims) if authorizer.validate(&claims) => claims,
        // The token is valid, it just does not carry the authority this route
        // asks for: RFC 6750 Section 3.1 `insufficient_scope`
        Ok(_) => {
            return Err(refusal(
                StatusCode::FORBIDDEN,
                "User does not have required role or permission",
                Some(authorizer::default_error_msg(metadata_url)),
            ));
        }
        // a server-side failure (unreachable OAuth issuer,
        // missing security key) is not the client's token
        // being at fault - no invalid_token challenge
        Err(err) if err.status().is_server_error() => {
            return Err(refusal(
                StatusCode::SERVICE_UNAVAILABLE,
                "Token validation is temporarily unavailable",
                None,
            ));
        }
        // The token itself did not hold up. Answer with the status its own
        // failure already carries - 401 for one that is malformed, expired or
        // signed by the wrong key (see `map_jwt_error_to_status`), and 400 where
        // the credential is not a bearer value at all, which is a failure of the
        // request rather than of a token. RFC 6750 Section 3.1 keeps 403 for a
        // token that is valid but does not carry enough authority, which is the
        // `Ok(_)` arm above. Answering 403 here would tell a client holding a
        // stale token that refreshing it cannot help, and it would disagree with
        // the `invalid_token` code the challenge below carries.
        Err(err) => {
            let status = err.status();
            let (message, challenge) = match err.inner.downcast_ref::<JwtError>() {
                Some(jwt) => (
                    describe_jwt_error(jwt.kind()).1,
                    build_www_authenticate(jwt.kind(), metadata_url),
                ),
                None => (
                    status.canonical_reason().unwrap_or("Invalid token"),
                    authorizer::default_error_msg(metadata_url),
                ),
            };
            return Err(refusal(status, message, Some(challenge)));
        }
    };

    if bts.strip_token_from_request {
        stash_bearer(&mut ctx, bearer);
        ctx.request_mut()
            .headers_mut()
            .remove(crate::headers::AUTHORIZATION);
    }
    ctx.request_mut()
        .extensions_mut()
        .insert(Authenticated(claims));

    next(ctx).await.map(|mut resp| {
        resp.headers_mut()
            .insert(CACHE_CONTROL, HeaderValue::from_static(NO_STORE));
        resp
    })
}

/// A request `authorize` refuses: an error with `status` and `message`, answered with
/// `challenge` as its `WWW-Authenticate` header and with `Cache-Control: no-store` by
/// whichever error handler renders it - see [`Error::with_header`]
#[inline]
#[cfg(feature = "jwt-auth")]
fn refusal(status: StatusCode, message: &'static str, challenge: Option<String>) -> Error {
    let err = Error::from_parts(status, None, message);
    let err = match challenge {
        Some(challenge) => err.with_header((WWW_AUTHENTICATE, challenge)),
        None => err,
    };
    err.with_header((CACHE_CONTROL, HeaderValue::from_static(NO_STORE)))
}

/// Returns the bearer token for this request, preferring the value stashed
/// by an outer `authorize` middleware (when `strip_token_from_request` removed
/// the `Authorization` header) over the raw header. This keeps stacked
/// `authorize` chains (e.g. group-level + route-level) working when the outer
/// step has already stripped the credential.
#[cfg(feature = "jwt-auth")]
fn resolve_bearer(ctx: &HttpContext) -> Result<Bearer, Error> {
    use crate::http::request_scope::HttpRequestScope;
    if let Some(scope) = ctx.request().extensions().get::<HttpRequestScope>()
        && let Some(bearer) = scope.bearer.as_ref()
    {
        return Ok(bearer.clone());
    }
    ctx.extract::<Bearer>()
}

/// Stashes the bearer token into the request scope so nested `authorize`
/// middlewares can recover it after the `Authorization` header is removed.
/// Idempotent: a token already present in the scope is left untouched.
#[cfg(feature = "jwt-auth")]
fn stash_bearer(ctx: &mut HttpContext, bearer: Bearer) {
    use crate::http::request_scope::HttpRequestScope;
    if let Some(scope) = ctx
        .request_mut()
        .extensions_mut()
        .get_mut::<HttpRequestScope>()
        && scope.bearer.is_none()
    {
        scope.bearer = Some(bearer);
    }
}

/// Returns `Ok(true)` when the request should be rejected because
/// `require_https` is enabled, the server isn't accepting TLS, and the
/// peer is not a loopback address.
#[cfg(feature = "jwt-auth")]
fn should_reject_for_https(ctx: &HttpContext, bts: &BearerTokenService) -> Result<bool, Error> {
    if !bts.require_https || bts.tls_enabled {
        return Ok(false);
    }
    let client_ip: crate::ClientIp = ctx.extract()?;
    Ok(!client_ip.into_inner().ip().is_loopback())
}

#[cfg(all(test, feature = "jwt-auth"))]
mod tests {
    use super::*;
    use crate::http::StatusCode;
    use jsonwebtoken::errors::{Error as JwtError, ErrorKind};

    /// Produces a genuine `ErrorKind::Base64` error.
    ///
    /// The variant wraps `jsonwebtoken`'s own `base64` version, which the crate
    /// does not re-export, so it cannot be constructed here directly - decoding a
    /// header with a non-base64 segment makes `jsonwebtoken` build it for us.
    fn base64_jwt_error() -> JwtError {
        let err = jsonwebtoken::decode_header("!!!!.eyJzdWIiOiIxIn0.sig")
            .expect_err("a non-base64 header segment must fail to decode");
        assert!(matches!(err.kind(), ErrorKind::Base64(_)));
        err
    }

    #[test]
    fn it_maps_expired_signature_to_unauthorized() {
        let status = map_jwt_error_to_status(&ErrorKind::ExpiredSignature);
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn it_maps_invalid_token_to_unauthorized() {
        let status = map_jwt_error_to_status(&ErrorKind::InvalidToken);
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn it_maps_invalid_issuer_to_unauthorized() {
        let status = map_jwt_error_to_status(&ErrorKind::InvalidIssuer);
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn it_maps_invalid_audience_to_unauthorized() {
        let status = map_jwt_error_to_status(&ErrorKind::InvalidAudience);
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn it_maps_invalid_subject_to_unauthorized() {
        let status = map_jwt_error_to_status(&ErrorKind::InvalidSubject);
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn it_maps_invalid_signature_to_unauthorized() {
        let status = map_jwt_error_to_status(&ErrorKind::InvalidSignature);
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn it_maps_missing_required_claim_to_unauthorized() {
        let status = map_jwt_error_to_status(&ErrorKind::MissingRequiredClaim("test".to_string()));
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn it_maps_immature_signature_to_unauthorized() {
        let status = map_jwt_error_to_status(&ErrorKind::ImmatureSignature);
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn it_maps_invalid_algorithm_name_to_unauthorized() {
        let status = map_jwt_error_to_status(&ErrorKind::InvalidAlgorithmName);
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn it_maps_invalid_algorithm_to_unauthorized() {
        let status = map_jwt_error_to_status(&ErrorKind::InvalidAlgorithm);
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn it_maps_base64_error_to_unauthorized() {
        let status = map_jwt_error_to_status(base64_jwt_error().kind());
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn it_maps_json_error_to_unauthorized() {
        // Create a JSON error by attempting to deserialize invalid JSON
        let json_result: Result<serde_json::Value, _> = serde_json::from_str("invalid json");
        let json_error = json_result.unwrap_err();
        let status = map_jwt_error_to_status(&ErrorKind::Json(Arc::from(json_error)));
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn it_maps_utf8_error_to_unauthorized() {
        // Create a FromUtf8Error by attempting to convert invalid UTF-8 bytes
        let invalid_utf8_bytes = vec![0, 159, 146, 150];
        let utf8_error = String::from_utf8(invalid_utf8_bytes).unwrap_err();
        let status = map_jwt_error_to_status(&ErrorKind::Utf8(utf8_error));
        assert_eq!(status, StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn it_maps_invalid_key_format_to_server_error() {
        let status = map_jwt_error_to_status(&ErrorKind::InvalidKeyFormat);
        assert_eq!(status, StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[test]
    fn it_maps_expired_signature_to_www_authenticate() {
        let www_auth = build_www_authenticate(&ErrorKind::ExpiredSignature, None);
        assert_eq!(
            www_auth,
            r#"Bearer error="invalid_token", error_description="Token has expired""#
        );
    }

    #[test]
    fn it_maps_invalid_signature_to_www_authenticate() {
        let www_auth = build_www_authenticate(&ErrorKind::InvalidSignature, None);
        assert_eq!(
            www_auth,
            r#"Bearer error="invalid_token", error_description="Invalid signature""#
        );
    }

    #[test]
    fn it_maps_invalid_token_to_www_authenticate() {
        let www_auth = build_www_authenticate(&ErrorKind::InvalidToken, None);
        assert_eq!(
            www_auth,
            r#"Bearer error="invalid_token", error_description="Token is malformed or invalid""#
        );
    }

    #[test]
    fn it_maps_immature_signature_to_www_authenticate() {
        let www_auth = build_www_authenticate(&ErrorKind::ImmatureSignature, None);
        assert_eq!(
            www_auth,
            r#"Bearer error="invalid_token", error_description="Token is not valid yet (nbf)""#
        );
    }

    #[test]
    fn it_maps_missing_required_claim_to_www_authenticate() {
        let www_auth =
            build_www_authenticate(&ErrorKind::MissingRequiredClaim("test".to_string()), None);
        assert_eq!(
            www_auth,
            r#"Bearer error="invalid_token", error_description="Missing required claim""#
        );
    }

    #[test]
    fn it_maps_invalid_issuer_to_www_authenticate() {
        let www_auth = build_www_authenticate(&ErrorKind::InvalidIssuer, None);
        assert_eq!(
            www_auth,
            r#"Bearer error="invalid_token", error_description="Invalid issuer (iss)""#
        );
    }

    #[test]
    fn it_maps_invalid_audience_to_www_authenticate() {
        let www_auth = build_www_authenticate(&ErrorKind::InvalidAudience, None);
        assert_eq!(
            www_auth,
            r#"Bearer error="invalid_token", error_description="Invalid audience (aud)""#
        );
    }

    #[test]
    fn it_maps_invalid_subject_to_www_authenticate() {
        let www_auth = build_www_authenticate(&ErrorKind::InvalidSubject, None);
        assert_eq!(
            www_auth,
            r#"Bearer error="invalid_token", error_description="Invalid subject (sub)""#
        );
    }

    #[test]
    fn it_maps_invalid_algorithm_to_www_authenticate() {
        let www_auth = build_www_authenticate(&ErrorKind::InvalidAlgorithm, None);
        assert_eq!(
            www_auth,
            r#"Bearer error="invalid_token", error_description="Invalid algorithm""#
        );
    }

    #[test]
    fn it_maps_invalid_algorithm_name_to_www_authenticate() {
        let www_auth = build_www_authenticate(&ErrorKind::InvalidAlgorithmName, None);
        assert_eq!(
            www_auth,
            r#"Bearer error="invalid_token", error_description="Invalid algorithm""#
        );
    }

    #[test]
    fn it_maps_base64_error_to_www_authenticate() {
        let www_auth = build_www_authenticate(base64_jwt_error().kind(), None);
        assert_eq!(
            www_auth,
            r#"Bearer error="invalid_token", error_description="Token is not properly base64-encoded""#
        );
    }

    #[test]
    fn it_maps_json_error_to_www_authenticate() {
        let json_result: Result<serde_json::Value, _> = serde_json::from_str("invalid json");
        let json_error = json_result.unwrap_err();
        let www_auth = build_www_authenticate(&ErrorKind::Json(Arc::from(json_error)), None);
        assert_eq!(
            www_auth,
            r#"Bearer error="invalid_token", error_description="Token payload is not valid JSON""#
        );
    }

    #[test]
    fn it_maps_utf8_error_to_www_authenticate() {
        let invalid_utf8_bytes = vec![0, 159, 146, 150];
        let utf8_error = String::from_utf8(invalid_utf8_bytes).unwrap_err();
        let www_auth = build_www_authenticate(&ErrorKind::Utf8(utf8_error), None);
        assert_eq!(
            www_auth,
            r#"Bearer error="invalid_token", error_description="Token contains invalid UTF-8 characters""#
        );
    }

    #[test]
    fn it_maps_invalid_key_format_to_www_authenticate() {
        let www_auth = build_www_authenticate(&ErrorKind::InvalidKeyFormat, None);
        assert_eq!(
            www_auth,
            r#"Bearer error="server_error", error_description="Invalid key format""#
        );
    }

    #[test]
    fn it_appends_resource_metadata_url_to_challenge() {
        let www_auth = build_www_authenticate(
            &ErrorKind::ExpiredSignature,
            Some("https://api.example.com/.well-known/oauth-protected-resource"),
        );
        assert_eq!(
            www_auth,
            r#"Bearer error="invalid_token", error_description="Token has expired", resource_metadata="https://api.example.com/.well-known/oauth-protected-resource""#
        );
    }

    #[test]
    fn it_default_error_msg_without_metadata_url() {
        let www_auth = authorizer::default_error_msg(None);
        assert_eq!(
            www_auth,
            r#"Bearer error="insufficient_scope", error_description="User does not have required role or permission""#
        );
    }

    #[test]
    fn it_default_error_msg_appends_metadata_url() {
        let www_auth = authorizer::default_error_msg(Some(
            "https://api.example.com/.well-known/oauth-protected-resource",
        ));
        assert_eq!(
            www_auth,
            r#"Bearer error="insufficient_scope", error_description="User does not have required role or permission", resource_metadata="https://api.example.com/.well-known/oauth-protected-resource""#
        );
    }

    #[test]
    fn it_converts_jwt_error_to_error_with_expired_signature() {
        let jwt_error = JwtError::from(ErrorKind::ExpiredSignature);
        let error = Error::from_jwt_error(jwt_error);

        assert_eq!(error.status, StatusCode::UNAUTHORIZED);
        assert!(error.instance().is_none());
    }

    #[test]
    fn it_converts_jwt_error_to_error_with_invalid_token() {
        let jwt_error = JwtError::from(ErrorKind::InvalidToken);
        let error = Error::from_jwt_error(jwt_error);

        assert_eq!(error.status, StatusCode::UNAUTHORIZED);
        assert!(error.instance().is_none());
    }

    #[test]
    fn it_converts_jwt_error_to_error_with_base64_error() {
        let error = Error::from_jwt_error(base64_jwt_error());

        assert_eq!(error.status, StatusCode::UNAUTHORIZED);
        assert!(error.instance().is_none());
    }

    #[test]
    fn it_converts_jwt_error_to_error_with_json_error() {
        let json_result: Result<serde_json::Value, _> = serde_json::from_str("invalid json");
        let json_error = json_result.unwrap_err();
        let jwt_error = JwtError::from(ErrorKind::Json(Arc::from(json_error)));
        let error = Error::from_jwt_error(jwt_error);

        assert_eq!(error.status, StatusCode::UNAUTHORIZED);
        assert!(error.instance().is_none());
    }

    #[test]
    fn it_converts_jwt_error_to_error_with_invalid_key_format() {
        let jwt_error = JwtError::from(ErrorKind::InvalidKeyFormat);
        let error = Error::from_jwt_error(jwt_error);

        assert_eq!(error.status, StatusCode::INTERNAL_SERVER_ERROR);
        assert!(error.instance().is_none());
    }

    #[test]
    fn it_sets_bearer_auth() {
        let app = App::new().set_bearer_auth(BearerAuthConfig::default());

        assert!(app.auth_config.is_some());
    }

    #[test]
    #[cfg(feature = "oauth-client")]
    fn it_sets_oauth() {
        let app =
            App::new().set_oauth(OAuthConfig::default().with_issuer("https://auth.example.com"));

        assert_eq!(
            app.oauth_client_config.and_then(|oauth| oauth.issuer),
            Some("https://auth.example.com".to_owned())
        );
    }

    #[test]
    #[cfg(feature = "oauth-client")]
    #[should_panic(
        expected = "OAuth is not configured; call `with_oauth(..)` or `set_oauth(..)` first"
    )]
    fn it_names_both_setters_when_use_oauth_finds_no_config() {
        let mut app = App::new();
        app.use_oauth();
    }

    #[test]
    #[cfg(feature = "oauth-client")]
    #[should_panic(
        expected = "OAuth issuer is not configured; call `with_oauth(|oauth| oauth.with_issuer(..))` \
        or `set_oauth(..)` with an issuer first"
    )]
    fn it_names_both_setters_when_use_oauth_finds_no_issuer() {
        let mut app = App::new().set_oauth(OAuthConfig::default());
        app.use_oauth();
    }
}
