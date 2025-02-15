﻿//! Extractors for [`CancellationToken`]

use tokio_util::sync::CancellationToken as TokioCancellationToken;
use futures_util::future::{ok, Ready};
use hyper::http::{request::Parts, Extensions};
use std::ops::{Deref, DerefMut};

use crate::{
    error::Error,
    HttpRequest,
    http::endpoints::args::{
        Source, 
        FromPayload, 
        FromRequestRef, 
        FromRequestParts, 
        Payload
    }
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

impl From<&Extensions> for TokenGuard {
    #[inline]
    fn from(extensions: &Extensions) -> Self {
        let token = extensions
            .get::<TokioCancellationToken>()
            .cloned()
            .unwrap_or_else(TokioCancellationToken::new);
        Self::new(token)
    }
}

impl From<&Parts> for TokenGuard {
    #[inline]
    fn from(parts: &Parts) -> Self {
        TokenGuard::from(&parts.extensions)
    }
}

/// Extracts `CancellationToken` from request parts
impl FromRequestParts for TokenGuard {
    #[inline]
    fn from_parts(parts: &Parts) -> Result<Self, Error> {
        Ok(parts.into())
    }
}

/// Extracts `CancellationToken` from request
impl FromRequestRef for TokenGuard {
    #[inline]
    fn from_request(req: &HttpRequest) -> Result<Self, Error> {
        Ok(req.extensions().into())
    }
}

/// Extracts `CancellationToken` from request parts
impl FromPayload for TokenGuard {
    type Future = Ready<Result<Self, Error>>;

    #[inline]
    fn from_payload(payload: Payload) -> Self::Future {
        let Payload::Parts(parts) = payload else { unreachable!() };
        ok(parts.into())
    }

    #[inline]
    fn source() -> Source {
        Source::Parts
    }
}

#[cfg(test)]
mod tests {
    use hyper::{Request, http::Extensions};
    use tokio_util::sync::CancellationToken as TokioCancellationToken;
    use crate::http::endpoints::args::{FromPayload, Payload};
    use crate::http::endpoints::args::cancellation_token::TokenGuard;

    #[tokio::test]
    async fn it_reads_from_payload() {
        let token = TokioCancellationToken::new();
        let req = Request::get("/")
            .extension(token.clone())
            .body(())
            .unwrap();

        token.cancel();

        let (parts, _) = req.into_parts();
        let token = TokenGuard::from_payload(Payload::Parts(&parts)).await.unwrap();

        assert!(token.is_cancelled());
    }

    #[test]
    fn it_gets_from_extensions() {
        let token = TokioCancellationToken::new();
        let mut extensions = Extensions::new();
        extensions.insert(token.clone());
        
        token.cancel();

        let token = TokenGuard::from(&extensions);
        
        assert!(token.is_cancelled());
    }

    #[test]
    fn it_gets_new_from_extensions_if_missing() {
        let extensions = Extensions::new();

        let token = TokenGuard::from(&extensions);

        assert!(!token.is_cancelled());
    }
}