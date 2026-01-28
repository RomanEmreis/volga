//! Authentication primitives.
//!
//! This module defines authentication extractors and traits used
//! by the framework to validate incoming requests and expose
//! user-defined authentication claims to handlers.

use super::claims::AuthClaims;
use std::ops::Deref;
use futures_util::future::{ready, Ready};
use hyper::http::{request::Parts, Extensions};
use crate::{
    http::{FromRequestParts, FromRequestRef, endpoints::args::{FromPayload, Payload, Source}},
    error::Error,
    HttpRequest,
};

/// Extractor that enforces authentication for a handler.
///
/// Handlers that include `Authenticated<T>` in their signature
/// are only invoked if authentication succeeds.
///
/// # Example
/// ```no_run
/// use volga::{App, auth::{Authenticated, AuthClaims, roles}};
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
/// async fn handler(auth: Authenticated<MyClaims>) {
///     println!("{}", auth.role);
/// }
/// ```
#[derive(Clone)]
pub struct Authenticated<T: AuthClaims>(pub(crate) T);

impl<T: AuthClaims> std::fmt::Debug for Authenticated<T> {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_tuple("Authenticated")
            .field(&"[redacted]")
            .finish()
    }
}

impl<T: AuthClaims> Deref for Authenticated<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T: AuthClaims> Authenticated<T> {
    /// Unwraps the inner claims
    #[inline]
    pub fn into_inner(self) -> T {
        self.0
    }

    /// Returns a reference to the authenticated claims.
    ///
    /// The returned claims are guaranteed to be valid and originate
    /// from a successfully authenticated request.
    #[inline]
    pub fn claims(&self) -> &T {
        &self.0
    }
}

impl<T> TryFrom<&Extensions> for Authenticated<T>
where
    T: AuthClaims + Send + Sync + 'static
{
    type Error = Error;

    #[inline]
    fn try_from(extensions: &Extensions) -> Result<Self, Self::Error> {
        extensions.get()
            .cloned()
            .ok_or_else(|| Error::server_error("Client IP: missing"))
    }
}

impl<T> TryFrom<&Parts> for Authenticated<T>
where
    T: AuthClaims + Send + Sync + 'static
{
    type Error = Error;
    
    #[inline]
    fn try_from(parts: &Parts) -> Result<Self, Self::Error> {
        Self::try_from(&parts.extensions)
    }
}

/// Extracts `Authenticated<T>` from request parts
impl<T> FromRequestParts for Authenticated<T>
where
    T: AuthClaims + Send + Sync + 'static
{
    #[inline]
    fn from_parts(parts: &Parts) -> Result<Self, Error> {
        parts.try_into()
    }
}

/// Extracts `Authenticated<T>` from request
impl<T> FromRequestRef for Authenticated<T>
where
    T: AuthClaims + Send + Sync + 'static
{
    #[inline]
    fn from_request(req: &HttpRequest) -> Result<Self, Error> {
        req.extensions().try_into()
    }
}

/// Extracts `Authenticated<T>` from the request payload
impl<T> FromPayload for Authenticated<T>
where
    T: AuthClaims + Send + Sync + 'static
{
    type Future = Ready<Result<Self, Error>>;

    const SOURCE: Source = Source::Parts;
    
    #[inline]
    fn from_payload(payload: Payload<'_>) -> Self::Future {
        let Payload::Parts(parts) = payload else { unreachable!() };
        ready(parts.try_into())
    }
}

#[cfg(test)]
mod tests {
    use hyper::{Request, http::Extensions};
    use crate::http::endpoints::args::{FromPayload, FromRequestParts, FromRequestRef, Payload};
    use serde::{Serialize, Deserialize};
    use crate::{HttpBody, HttpRequest};
    use crate::claims;
    use super::*;

    claims! {
        #[derive(Clone, Debug, Serialize, Deserialize)]
        struct MyClaims {
            sub: String
        }
    }

    #[tokio::test]
    async fn it_reads_from_payload() {
        let auth = Authenticated(MyClaims {
            sub: "sub".to_string()
        });
        let req = Request::get("/")
            .extension(auth.clone())
            .body(())
            .unwrap();

        let (parts, _) = req.into_parts();
        let from_payload = Authenticated::<MyClaims>::from_payload(Payload::Parts(&parts))
            .await
            .unwrap();

        assert_eq!(from_payload.sub, auth.sub);
    }

    #[test]
    fn it_gets_from_extensions() {
        let auth = Authenticated(MyClaims {
            sub: "sub".to_string()
        });
        let mut extensions = Extensions::new();
        extensions.insert(auth.clone());

        let from_ext = Authenticated::<MyClaims>::try_from(&extensions).unwrap();

        assert_eq!(from_ext.sub, auth.sub);
    }

    #[test]
    fn it_gets_err_from_extensions_if_missing() {
        let extensions = Extensions::new();

        let auth = Authenticated::<MyClaims>::try_from(&extensions);

        assert!(auth.is_err());
    }

    #[test]
    fn it_gets_from_request_parts() {
        let auth = Authenticated(MyClaims {
            sub: "sub".to_string()
        });
        let req = Request::get("/")
            .extension(auth.clone())
            .body(())
            .unwrap();

        let (parts, _) = req.into_parts();
        let from_parts = Authenticated::<MyClaims>::from_parts(&parts).unwrap();

        assert_eq!(from_parts.sub, auth.sub);
    }

    #[test]
    fn it_gets_from_request_ref() {
        let auth = Authenticated(MyClaims {
            sub: "sub".to_string()
        });
        let req = Request::get("/")
            .extension(auth.clone())
            .body(HttpBody::empty())
            .unwrap();

        let (parts, body) = req.into_parts();
        let req = HttpRequest::from_parts(parts, body);

        let from_req = Authenticated::<MyClaims>::from_request(&req).unwrap();

        assert_eq!(from_req.sub, auth.sub);
    }

    #[test]
    fn it_debugs() {
        let auth = Authenticated(MyClaims {
            sub: "sub".to_string()
        });

        assert_eq!(format!("{auth:?}"), r#"Authenticated("[redacted]")"#);
    }
}