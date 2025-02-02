﻿//! Extractors for [`CancellationToken`]

use tokio_util::sync::CancellationToken as TokioCancellationToken;
use futures_util::future::{ready, Ready};
use hyper::http::Extensions;
use std::ops::{Deref, DerefMut};

use crate::{
    error::Error, HttpRequest,
    http::endpoints::args::{Source, FromPayload, FromRequestRef, Payload}
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
    pub fn new(cancellation_token: TokioCancellationToken) -> Self {
        Self(cancellation_token)
    }
    
    /// Unwraps the inner [`tokio_util::sync::CancellationToken`]
    #[inline]
    pub fn into_inner(self) -> TokioCancellationToken {
        self.0
    }
    
    #[inline]
    pub(crate) fn from_extensions(extensions: &Extensions) -> Result<Self, Error> {
        let token = extensions
            .get::<TokioCancellationToken>()
            .cloned()
            .unwrap_or_else(TokioCancellationToken::new);
        Ok(TokenGuard::new(token))
    }
}

impl Deref for TokenGuard {
    type Target = TokioCancellationToken;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for TokenGuard {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl Clone for TokenGuard {
    fn clone(&self) -> Self {
        Self::new(self.0.clone())
    }

    fn clone_from(&mut self, source: &Self) {
        self.0.clone_from(&source.0);
    }
}

/// Extracts `CancellationToken` from request
impl FromRequestRef for TokenGuard {
    #[inline]
    fn from_request(req: &HttpRequest) -> Result<Self, Error> {
        Self::from_extensions(req.extensions())
    }
}

/// Extracts `CancellationToken` from request parts
impl FromPayload for TokenGuard {
    type Future = Ready<Result<Self, Error>>;

    #[inline]
    fn from_payload(payload: Payload) -> Self::Future {
        if let Payload::Ext(extensions) = payload {
            ready(Self::from_extensions(extensions))
        } else {
            unreachable!()
        }
    }

    #[inline]
    fn source() -> Source {
        Source::Ext
    }
}

#[cfg(test)]
mod tests {
    use hyper::http::Extensions;
    use tokio_util::sync::CancellationToken as TokioCancellationToken;
    use crate::http::endpoints::args::{FromPayload, Payload};
    use crate::http::endpoints::args::cancellation_token::TokenGuard;

    #[tokio::test]
    async fn it_reads_from_payload() {
        let token = TokioCancellationToken::new();
        let mut extensions = Extensions::new();
        extensions.insert(token.clone());

        token.cancel();
        
        let token = TokenGuard::from_payload(Payload::Ext(&extensions)).await.unwrap();

        assert!(token.is_cancelled());
    }

    #[test]
    fn it_gets_from_extensions() {
        let token = TokioCancellationToken::new();
        let mut extensions = Extensions::new();
        extensions.insert(token.clone());
        
        token.cancel();
        
        let token = TokenGuard::from_extensions(&extensions).unwrap();
        
        assert!(token.is_cancelled());
    }

    #[test]
    fn it_gets_new_from_extensions_if_missing() {
        let extensions = Extensions::new();

        let token = TokenGuard::from_extensions(&extensions).unwrap();

        assert!(!token.is_cancelled());
    }
}