//! Extractors for [`CancellationToken`]

use futures_util::future::{Ready, ready};
use hyper::http::request::Parts;
use std::ops::{Deref, DerefMut};
use tokio_util::sync::CancellationToken as TokioCancellationToken;

use crate::{
    HttpRequest,
    error::Error,
    http::{
        endpoints::args::{FromPayload, FromRequestParts, FromRequestRef, Payload, Source},
        request_scope::HttpRequestScope,
    },
};

/// See [`tokio_util::sync::CancellationToken`] for more details.
pub type CancellationToken = TokenGuard;

/// Wraps the [`tokio_util::sync::CancellationToken`]
///
/// # Example
/// ```no_run
/// use volga::{HttpResult, CancellationToken, ok};
///
/// async fn handle(token: CancellationToken) -> HttpResult {
///     ok!("Token cancellation status: {}", token.is_cancelled())
/// }
/// ```
#[derive(Debug)]
pub struct TokenGuard(TokioCancellationToken);

impl TokenGuard {
    /// Creates a new instance of `TokenGuard`
    #[inline]
    pub fn new(cancellation_token: TokioCancellationToken) -> Self {
        Self(cancellation_token)
    }

    /// Unwraps the inner [`tokio_util::sync::CancellationToken`]
    #[inline]
    pub fn into_inner(self) -> TokioCancellationToken {
        self.0
    }
}

impl Deref for TokenGuard {
    type Target = TokioCancellationToken;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for TokenGuard {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Clone for TokenGuard {
    #[inline]
    fn clone(&self) -> Self {
        Self::new(self.0.clone())
    }

    #[inline]
    fn clone_from(&mut self, source: &Self) {
        self.0.clone_from(&source.0);
    }
}

/// Extracts `CancellationToken` from request parts
impl FromRequestParts for TokenGuard {
    #[inline]
    fn from_parts(parts: &Parts) -> Result<Self, Error> {
        parts
            .extensions
            .get::<HttpRequestScope>()
            .map(|s| Self::new(s.cancellation_token.clone()))
            .ok_or_else(|| Error::server_error("CancellationToken: missing"))
    }
}

/// Extracts `CancellationToken` from request
impl FromRequestRef for TokenGuard {
    #[inline]
    fn from_request(req: &HttpRequest) -> Result<Self, Error> {
        req.extensions()
            .get::<HttpRequestScope>()
            .map(|s| Self::new(s.cancellation_token.clone()))
            .ok_or_else(|| Error::server_error("CancellationToken: missing"))
    }
}

/// Extracts `CancellationToken` from request parts
impl FromPayload for TokenGuard {
    type Future = Ready<Result<Self, Error>>;

    const SOURCE: Source = Source::Parts;

    #[inline]
    fn from_payload(payload: Payload<'_>) -> Self::Future {
        let Payload::Parts(parts) = payload else {
            unreachable!()
        };
        ready(Self::from_parts(parts))
    }
}

#[cfg(test)]
mod tests {
    use crate::http::endpoints::args::cancellation_token::TokenGuard;
    use crate::http::endpoints::args::{FromPayload, FromRequestParts, FromRequestRef, Payload};
    use crate::http::request_scope::HttpRequestScope;
    use crate::{HttpBody, HttpRequest};
    use hyper::Request;
    use tokio_util::sync::CancellationToken as TokioCancellationToken;

    fn make_scope_with_token(token: TokioCancellationToken) -> HttpRequestScope {
        HttpRequestScope {
            cancellation_token: token,
            ..HttpRequestScope::default()
        }
    }

    #[tokio::test]
    async fn it_reads_from_payload() {
        let token = TokioCancellationToken::new();
        token.cancel();

        let req = Request::get("/")
            .extension(make_scope_with_token(token))
            .body(())
            .unwrap();

        let (parts, _) = req.into_parts();
        let token = TokenGuard::from_payload(Payload::Parts(&parts))
            .await
            .unwrap();

        assert!(token.is_cancelled());
    }

    #[test]
    fn it_gets_err_from_parts_if_scope_missing() {
        let req = Request::get("/").body(()).unwrap();
        let (parts, _) = req.into_parts();
        assert!(TokenGuard::from_parts(&parts).is_err());
    }

    #[test]
    fn it_gets_from_request_parts() {
        let token = TokioCancellationToken::new();
        token.cancel();

        let req = Request::get("/")
            .extension(make_scope_with_token(token))
            .body(())
            .unwrap();

        let (parts, _) = req.into_parts();
        let token = TokenGuard::from_parts(&parts).unwrap();

        assert!(token.is_cancelled());
    }

    #[test]
    fn it_gets_from_request_ref() {
        let token = TokioCancellationToken::new();
        token.cancel();

        let req = Request::get("/")
            .extension(make_scope_with_token(token))
            .body(HttpBody::empty())
            .unwrap();

        let (parts, body) = req.into_parts();
        let req = HttpRequest::from_parts(parts, body);

        let token = TokenGuard::from_request(&req).unwrap();

        assert!(token.is_cancelled());
    }

    #[test]
    fn it_derefs_mut() {
        let mut token = TokenGuard(TokioCancellationToken::new());

        token.cancel();

        *token = TokioCancellationToken::new();

        assert!(!token.is_cancelled());
    }

    #[test]
    fn it_clones() {
        let token = TokenGuard(TokioCancellationToken::new());
        token.cancel();

        let another_token = token.clone();

        assert!(another_token.is_cancelled());
    }

    #[test]
    fn it_clones_from() {
        let token = TokenGuard(TokioCancellationToken::new());
        token.cancel();

        let mut another_token = TokenGuard(TokioCancellationToken::new());
        another_token.clone_from(&token);

        assert!(another_token.is_cancelled());
    }
}
