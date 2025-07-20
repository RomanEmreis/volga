//! Tools and utils for Authorization & Authentication

#[cfg(feature = "jwt-auth")]
use {
    crate::{App, routing::{Route, RouteGroup}, http::StatusCode, error::Error, status, HttpResult},
    std::{future::Future, sync::Arc},
};

#[cfg(feature = "jwt-auth")]
pub use {
    jsonwebtoken::{Algorithm, EncodingKey, DecodingKey, errors::{ErrorKind, Error as JwtError}},
    authorizer::{Authorizer, AuthClaims, role, roles, permissions, predicate},
    crate::middleware::{HttpContext, NextFn},
    bearer::{BearerAuthConfig, Bearer, BearerTokenService},
    crate::headers::{HeaderValue, WWW_AUTHENTICATE}
};
#[cfg(feature = "basic-auth")]
pub use basic::Basic;

#[cfg(feature = "basic-auth")]
pub mod basic;
#[cfg(feature = "jwt-auth")]
pub mod bearer;
#[cfg(feature = "jwt-auth")]
pub mod authorizer;

#[cfg(feature = "jwt-auth")]
impl From<JwtError> for Error {
    #[inline]
    fn from(err: JwtError) -> Self {
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
        Base64(_)
        | Json(_)
        | Utf8(_)
        | Crypto(_)
        | InvalidKeyFormat => StatusCode::BAD_REQUEST,
        _ => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

#[cfg(feature = "jwt-auth")]
fn map_jwt_error_to_www_authenticate(err: &ErrorKind) -> &'static str {
    use ErrorKind::*;
    match err {
        ExpiredSignature => r#"Bearer error="invalid_token", error_description="Token has expired""#,
        InvalidSignature => r#"Bearer error="invalid_token", error_description="Invalid signature""#,
        InvalidToken => r#"Bearer error="invalid_token", error_description="Token is malformed or invalid""#,
        ImmatureSignature => r#"Bearer error="invalid_token", error_description="Token is not valid yet (nbf)""#,
        MissingRequiredClaim(_) => r#"Bearer error="invalid_token", error_description="Missing required claim""#,
        InvalidIssuer => r#"Bearer error="invalid_token", error_description="Invalid issuer (iss)""#,
        InvalidAudience => r#"Bearer error="invalid_token", error_description="Invalid audience (aud)""#,
        InvalidSubject => r#"Bearer error="invalid_token", error_description="Invalid subject (sub)""#,
        InvalidAlgorithm | InvalidAlgorithmName => r#"Bearer error="invalid_token", error_description="Invalid algorithm""#,
        Base64(_) => r#"Bearer error="invalid_request", error_description="Token is not properly base64-encoded""#,
        Json(_) => r#"Bearer error="invalid_request", error_description="Token payload is not valid JSON""#,
        Utf8(_) => r#"Bearer error="invalid_request", error_description="Token contains invalid UTF-8 characters""#,
        InvalidKeyFormat => r#"Bearer error="invalid_request", error_description="Invalid key format""#,
        Crypto(_) => r#"Bearer error="invalid_token", error_description="Cryptographic error during token validation""#,
        _ => r#"Bearer error="server_error", error_description="Internal token processing error""#,
    }
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
        F: FnOnce(BearerAuthConfig) -> BearerAuthConfig
    {
        self.auth_config = Some(config(Default::default()));
        self
    }

    /// Adds authorization middleware for all routes
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, auth::{AuthClaims, roles}};
    /// use serde::Deserialize;
    ///
    /// #[derive(Deserialize)]
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
    pub fn authorize<C: AuthClaims + Send +  Sync + 'static>(&mut self, authorizer: Authorizer<C>) -> &mut Self {
        self.ensure_bearer_auth_configured();
        let authorizer = Arc::new(authorizer);
        self.wrap(move |ctx, next| authorize_impl(authorizer.clone(), ctx, next))
    }
    
    fn ensure_bearer_auth_configured(&self) {
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
    /// #[derive(Deserialize)]
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
    pub fn authorize<C: AuthClaims + Send +  Sync + 'static>(self, authorizer: Authorizer<C>) -> Self {
        self.ensure_bearer_auth_configured();
        let authorizer = Arc::new(authorizer);
        self.wrap(move |ctx, next| authorize_impl(authorizer.clone(), ctx, next))
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
    /// #[derive(Deserialize)]
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
    /// app.map_group("/greet")
    ///     .authorize::<MyClaims>(roles(["admin", "user"]))
    ///     .map_get("/hello", || async { "Hello, World!" });
    /// ```
    pub fn authorize<C: AuthClaims + Send +  Sync + 'static>(self, authorizer: Authorizer<C>) -> Self {
        self.app.ensure_bearer_auth_configured();
        let authorizer = Arc::new(authorizer);
        self.wrap(move |ctx, next| authorize_impl(authorizer.clone(), ctx, next))
    }
}

#[cfg(feature = "jwt-auth")]
fn authorize_impl<C>(authorizer: Arc<Authorizer<C>>, ctx: HttpContext, next: NextFn) -> impl Future<Output = HttpResult>
where
    C: AuthClaims + Send +  Sync + 'static
{
    let authorizer = authorizer.clone();
    async move {
        let bearer: Bearer = ctx.extract()?;
        let bts: BearerTokenService = ctx.extract()?;
        match bts.decode(bearer) {
            Ok(claims) if authorizer.validate(&claims) => next(ctx).await,
            Ok(_) => status!(403, [
                (WWW_AUTHENTICATE, authorizer::DEFAULT_ERROR_MSG)
            ]),
            Err(err) => {
                let www_authenticate = err
                    .into_inner()
                    .downcast_ref::<JwtError>()
                    .map(|e| map_jwt_error_to_www_authenticate(e.kind()))
                    .unwrap_or(authorizer::DEFAULT_ERROR_MSG);
                status!(403, [
                    (WWW_AUTHENTICATE, www_authenticate)
                ])
            }
        }
    }
}

