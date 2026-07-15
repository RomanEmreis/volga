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
        status,
    },
    std::{future::Future, sync::Arc},
};

#[cfg(feature = "jwt-auth")]
pub use {
    algorithm::Algorithm,
    authenticated::Authenticated,
    authorizer::{Authorizer, permissions, predicate, role, roles},
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
#[cfg(feature = "oauth")]
pub mod oauth;
#[cfg(feature = "oauth-client")]
pub mod oauth_client;
#[cfg(feature = "jwt-auth")]
pub(crate) mod pem;

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
        | InvalidAlgorithm => StatusCode::UNAUTHORIZED,
        Base64(_) | Json(_) | Utf8(_) | InvalidKeyFormat => StatusCode::BAD_REQUEST,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

#[cfg(feature = "jwt-auth")]
fn build_www_authenticate(err: &ErrorKind, resource_metadata_url: Option<&str>) -> String {
    use crate::auth::oauth::OAuthErrorCode::{
        InvalidRequest, InvalidToken as InvalidTokenCode, ServerError,
    };
    use ErrorKind::*;

    let (code, description) = match err {
        ExpiredSignature => (InvalidTokenCode, "Token has expired"),
        InvalidSignature => (InvalidTokenCode, "Invalid signature"),
        InvalidToken => (InvalidTokenCode, "Token is malformed or invalid"),
        ImmatureSignature => (InvalidTokenCode, "Token is not valid yet (nbf)"),
        MissingRequiredClaim(_) => (InvalidTokenCode, "Missing required claim"),
        InvalidIssuer => (InvalidTokenCode, "Invalid issuer (iss)"),
        InvalidAudience => (InvalidTokenCode, "Invalid audience (aud)"),
        InvalidSubject => (InvalidTokenCode, "Invalid subject (sub)"),
        InvalidAlgorithm | InvalidAlgorithmName => (InvalidTokenCode, "Invalid algorithm"),
        Base64(_) => (InvalidRequest, "Token is not properly base64-encoded"),
        Json(_) => (InvalidRequest, "Token payload is not valid JSON"),
        Utf8(_) => (InvalidRequest, "Token contains invalid UTF-8 characters"),
        InvalidKeyFormat => (InvalidRequest, "Invalid key format"),
        _ => (ServerError, "Internal token processing error"),
    };

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
    pub fn with_bearer_auth<F>(mut self, config: F) -> Self
    where
        F: FnOnce(BearerAuthConfig) -> BearerAuthConfig,
    {
        self.auth_config = Some(config(Default::default()));
        self
    }

    /// Describes the OAuth 2.1/OIDC issuer whose keys validate bearer
    /// tokens — activate it explicitly with [`use_oauth`](App::use_oauth)
    ///
    /// The issuer metadata and its JSON Web Key Set are fetched lazily on
    /// the first request and refreshed on key rotation; see
    /// [`OAuthConfig`] for the knobs. Token checks other than the key and
    /// `iss` (audience, expiry, …) stay on
    /// [`with_bearer_auth`](App::with_bearer_auth).
    ///
    /// With the `config` feature the same knobs can come from the
    /// `[oauth.client]` section of the configuration file instead —
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
    pub fn with_oauth<F>(mut self, config: F) -> Self
    where
        F: FnOnce(OAuthConfig) -> OAuthConfig,
    {
        self.oauth_client_config = Some(config(Default::default()));
        self
    }

    /// Explicitly enables validation of bearer tokens against the OAuth
    /// issuer configured with [`with_oauth`](App::with_oauth)
    ///
    /// # Panics
    /// Panics when [`with_oauth`](App::with_oauth) was not called or did
    /// not configure an issuer.
    #[cfg(feature = "oauth-client")]
    pub fn use_oauth(&mut self) -> &mut Self {
        match &self.oauth_client_config {
            Some(config) if config.issuer.is_some() => self.oauth_client_enabled = true,
            Some(_) => panic!(
                "OAuth issuer is not configured; call `with_oauth(|oauth| oauth.with_issuer(..))` first"
            ),
            None => panic!("OAuth is not configured; call `with_oauth(..)` first"),
        }
        self
    }

    /// Adds authorization middleware for all routes
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
        // issuer-based validation resolves keys at runtime — no static
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
fn authorize_impl<C>(
    authorizer: Arc<Authorizer<C>>,
    mut ctx: HttpContext,
    next: NextFn,
) -> impl Future<Output = HttpResult>
where
    C: AuthClaims + Send + Sync + 'static,
{
    let authorizer = authorizer.clone();
    async move {
        if should_reject_for_https(&ctx)? {
            return status!(400; [
                (WWW_AUTHENTICATE, r#"Bearer error="invalid_request", error_description="HTTPS required""#)
            ]);
        }
        let bts: BearerTokenService = ctx.extract()?;
        let resp = match resolve_bearer(&ctx) {
            Err(_) => {
                let mut challenge = oauth::BearerChallenge::new();
                if let Some(url) = bts.resource_metadata_url.as_deref() {
                    challenge = challenge.with_resource_metadata(url);
                }
                if ctx
                    .request()
                    .headers()
                    .contains_key(crate::headers::AUTHORIZATION)
                {
                    // RFC 6750 §3.1: credentials were presented but are not
                    // a well-formed Bearer value (wrong scheme, empty token)
                    // — the client should fix the header, not start a flow
                    let challenge = challenge
                        .with_error(oauth::OAuthErrorCode::InvalidRequest)
                        .with_description("Authorization header is not a valid Bearer credential");

                    status!(400; [
                        (WWW_AUTHENTICATE, challenge.to_string())
                    ])
                } else {
                    // RFC 6750 §3: no credentials were presented — challenge
                    // with a bare scheme (no error code) so clients can
                    // discover the resource metadata and start an
                    // authorization flow
                    status!(401; [
                        (WWW_AUTHENTICATE, challenge.to_string())
                    ])
                }
            }
            Ok(bearer) => {
                #[cfg(feature = "oauth-client")]
                let decoded = bts.decode_async(bearer.clone()).await;
                #[cfg(not(feature = "oauth-client"))]
                let decoded = bts.decode(bearer.clone());
                match decoded {
                    Ok(claims) if authorizer.validate(&claims) => {
                        if bts.strip_token_from_request {
                            stash_bearer(&mut ctx, bearer);
                            ctx.request_mut()
                                .headers_mut()
                                .remove(crate::headers::AUTHORIZATION);
                        }
                        ctx.request_mut()
                            .extensions_mut()
                            .insert(Authenticated(claims));

                        next(ctx).await
                    }
                    Ok(_) => {
                        let metadata_url = bts.resource_metadata_url.as_deref();

                        status!(403; [
                            (WWW_AUTHENTICATE, authorizer::default_error_msg(metadata_url))
                        ])
                    }
                    // a server-side failure (unreachable OAuth issuer,
                    // missing security key) is not the client's token
                    // being at fault — no invalid_token challenge
                    Err(err) if err.status().is_server_error() => {
                        status!(503, "Token validation is temporarily unavailable")
                    }
                    Err(err) => {
                        let metadata_url = bts.resource_metadata_url.as_deref();
                        let www_authenticate = err
                            .into_inner()
                            .downcast_ref::<JwtError>()
                            .map(|e| build_www_authenticate(e.kind(), metadata_url))
                            .unwrap_or_else(|| authorizer::default_error_msg(metadata_url));

                        status!(403; [
                            (WWW_AUTHENTICATE, www_authenticate)
                        ])
                    }
                }
            }
        };
        resp.map(|mut resp| {
            resp.headers_mut()
                .insert(CACHE_CONTROL, HeaderValue::from_static(NO_STORE));
            resp
        })
    }
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
fn should_reject_for_https(ctx: &HttpContext) -> Result<bool, Error> {
    let bts: BearerTokenService = ctx.extract()?;
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
    fn it_maps_base64_error_to_bad_request() {
        let status =
            map_jwt_error_to_status(&ErrorKind::Base64(base64::DecodeError::InvalidByte(0, 0)));
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn it_maps_json_error_to_bad_request() {
        // Create a JSON error by attempting to deserialize invalid JSON
        let json_result: Result<serde_json::Value, _> = serde_json::from_str("invalid json");
        let json_error = json_result.unwrap_err();
        let status = map_jwt_error_to_status(&ErrorKind::Json(Arc::from(json_error)));
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn it_maps_utf8_error_to_bad_request() {
        // Create a FromUtf8Error by attempting to convert invalid UTF-8 bytes
        let invalid_utf8_bytes = vec![0, 159, 146, 150];
        let utf8_error = String::from_utf8(invalid_utf8_bytes).unwrap_err();
        let status = map_jwt_error_to_status(&ErrorKind::Utf8(utf8_error));
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[test]
    fn it_maps_invalid_key_format_to_bad_request() {
        let status = map_jwt_error_to_status(&ErrorKind::InvalidKeyFormat);
        assert_eq!(status, StatusCode::BAD_REQUEST);
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
        let www_auth = build_www_authenticate(
            &ErrorKind::Base64(base64::DecodeError::InvalidByte(0, 0)),
            None,
        );
        assert_eq!(
            www_auth,
            r#"Bearer error="invalid_request", error_description="Token is not properly base64-encoded""#
        );
    }

    #[test]
    fn it_maps_json_error_to_www_authenticate() {
        let json_result: Result<serde_json::Value, _> = serde_json::from_str("invalid json");
        let json_error = json_result.unwrap_err();
        let www_auth = build_www_authenticate(&ErrorKind::Json(Arc::from(json_error)), None);
        assert_eq!(
            www_auth,
            r#"Bearer error="invalid_request", error_description="Token payload is not valid JSON""#
        );
    }

    #[test]
    fn it_maps_utf8_error_to_www_authenticate() {
        let invalid_utf8_bytes = vec![0, 159, 146, 150];
        let utf8_error = String::from_utf8(invalid_utf8_bytes).unwrap_err();
        let www_auth = build_www_authenticate(&ErrorKind::Utf8(utf8_error), None);
        assert_eq!(
            www_auth,
            r#"Bearer error="invalid_request", error_description="Token contains invalid UTF-8 characters""#
        );
    }

    #[test]
    fn it_maps_invalid_key_format_to_www_authenticate() {
        let www_auth = build_www_authenticate(&ErrorKind::InvalidKeyFormat, None);
        assert_eq!(
            www_auth,
            r#"Bearer error="invalid_request", error_description="Invalid key format""#
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
        assert!(error.instance.is_none());
    }

    #[test]
    fn it_converts_jwt_error_to_error_with_invalid_token() {
        let jwt_error = JwtError::from(ErrorKind::InvalidToken);
        let error = Error::from_jwt_error(jwt_error);

        assert_eq!(error.status, StatusCode::UNAUTHORIZED);
        assert!(error.instance.is_none());
    }

    #[test]
    fn it_converts_jwt_error_to_error_with_base64_error() {
        let jwt_error = JwtError::from(ErrorKind::Base64(base64::DecodeError::InvalidByte(0, 0)));
        let error = Error::from_jwt_error(jwt_error);

        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert!(error.instance.is_none());
    }

    #[test]
    fn it_converts_jwt_error_to_error_with_json_error() {
        let json_result: Result<serde_json::Value, _> = serde_json::from_str("invalid json");
        let json_error = json_result.unwrap_err();
        let jwt_error = JwtError::from(ErrorKind::Json(Arc::from(json_error)));
        let error = Error::from_jwt_error(jwt_error);

        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert!(error.instance.is_none());
    }

    #[test]
    fn it_converts_jwt_error_to_error_with_invalid_key_format() {
        let jwt_error = JwtError::from(ErrorKind::InvalidKeyFormat);
        let error = Error::from_jwt_error(jwt_error);

        assert_eq!(error.status, StatusCode::BAD_REQUEST);
        assert!(error.instance.is_none());
    }
}
