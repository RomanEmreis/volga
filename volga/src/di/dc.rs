//! Extractors for Dependency Injection

use super::{Container, Inject};
use futures_util::future::{ready, Ready};
use hyper::http::{request::Parts, Extensions};
use crate::{
    error::Error, http::endpoints::args::{
        FromRequestRef,
        FromRequestParts,
        FromPayload,
        Payload,
        Source
    },
    HttpRequest
};

use std::{
    ops::{Deref, DerefMut},
    sync::Arc
};

/// `Dc` stands for Dependency Container, This struct wraps the injectable type of `T` 
/// `T` must be registered in Dependency Injection Container
/// 
/// # Example
/// ```no_run
/// use volga::{App, di::Dc, ok, not_found};
/// use std::{
///     collections::HashMap,
///     sync::{Arc, Mutex}
/// };
/// 
/// #[derive(Clone, Default)]
/// struct InMemoryCache {
///     inner: Arc<Mutex<HashMap<String, String>>>
/// }
/// 
///# #[tokio::main]
///# async fn main() -> std::io::Result<()> {
/// let mut app = App::new();
/// 
/// app.add_singleton(InMemoryCache::default());
/// 
/// app.map_get("/user/{id}", |id: String, cache: Dc<InMemoryCache>| async move {
///     let cache_guard = cache.inner.lock().unwrap();
///     let user = cache_guard.get(&id);
///     match user { 
///         Some(user) => ok!(user),
///         None => not_found!()
///     }
/// });
///# app.run().await
///# }
/// ```
#[derive(Debug, Default, Clone)]
pub struct Dc<T: Inject>(Arc<T>);

impl<T: Inject> Deref for Dc<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T: Inject + Clone> DerefMut for Dc<T> {
    fn deref_mut(&mut self) -> &mut T {
        Arc::make_mut(&mut self.0)
    }
}

impl<T: Inject> Dc<T> {
    /// Unwraps the inner [`Arc`]
    pub fn into_inner(self) -> Arc<T> {
        self.0
    }
}

impl<T: Inject + Clone> Dc<T> {
    /// Returns a clone of the inner `T` if it implements [`Clone`]
    #[inline]
    pub fn cloned(&self) -> T {
        self.0.as_ref().clone()
    }
}

impl<T: Inject + 'static> TryFrom<&Extensions> for Dc<T> {
    type Error = Error;
    
    #[inline]
    fn try_from(extensions: &Extensions) -> Result<Self, Self::Error> {
        Container::try_from(extensions)?
            .resolve_shared::<T>()
            .map_err(Into::into)
            .map(Dc)
    }
}

impl<T: Inject + 'static> FromRequestRef for Dc<T> {
    #[inline]
    fn from_request(req: &HttpRequest) -> Result<Self, Error> {
        req.extensions().try_into()
    }    
}

impl<T: Inject + 'static> FromRequestParts for Dc<T> {
    #[inline]
    fn from_parts(parts: &Parts) -> Result<Self, Error> {
        let ext = &parts.extensions;
        ext.try_into()
    }
}

impl<T: Inject + 'static> FromPayload for Dc<T> {
    type Future = Ready<Result<Self, Error>>;

    #[inline]
    fn from_payload(payload: Payload) -> Self::Future {
        let Payload::Parts(parts) = payload else { unreachable!() };
        ready(Self::from_parts(parts))
    }

    fn source() -> Source {
        Source::Parts
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use hyper::http::Extensions;
    use hyper::Request;
    use super::Dc;
    use crate::di::ContainerBuilder;
    use crate::http::endpoints::args::{FromPayload, FromRequestRef, FromRequestParts, Payload};
    use crate::{HttpBody, HttpRequest};

    type Cache = Arc<Mutex<Vec<i32>>>;
    
    #[tokio::test]
    async fn it_reads_from_payload() {
        let mut container = ContainerBuilder::new();
        
        container.register_scoped::<Cache>();
        
        let container = container.build();
        
        let scope = container.create_scope();
        let vec = scope.resolve::<Cache>().unwrap();
        vec.lock().unwrap().push(1);
        
        let mut req = Request::get("/").body(()).unwrap();
        req.extensions_mut().insert(scope);
            
        let (parts, _) = req.into_parts();
        
        let dc = Dc::<Cache>::from_payload(Payload::Parts(&parts)).await.unwrap();
        dc.lock().unwrap().push(2);

        let dc = Dc::<Cache>::from_payload(Payload::Parts(&parts)).await.unwrap();
        dc.lock().unwrap().push(3);

        let dc = Dc::<Cache>::from_payload(Payload::Parts(&parts)).await.unwrap();
        let dc = dc.lock().unwrap();
        
        assert_eq!(dc[0], 1);
        assert_eq!(dc[1], 2);
        assert_eq!(dc[2], 3);
    }

    #[test]
    fn it_tries_to_extract_from_extensions() {
        let mut container = ContainerBuilder::new();
        container.register_singleton::<Cache>(Cache::default());
        let container = container.build();

        let vec = container.resolve::<Cache>().unwrap();
        vec.lock().unwrap().push(1);

        let mut extensions = Extensions::new();
        extensions.insert(container);

        let dc = Dc::<Cache>::try_from(&extensions).unwrap();

        assert_eq!(dc.lock().unwrap().first().cloned(), Some(1));
    }

    #[test]
    fn it_tries_to_extract_from_extensions_and_fails_without_container() {
        let extensions = Extensions::new();

        let result = Dc::<Cache>::try_from(&extensions);

        assert!(result.is_err());
    }

    #[test]
    fn it_extracts_from_request_ref() {
        // Setup
        let mut container = ContainerBuilder::new();
        container.register_singleton::<Cache>(Cache::default());
        let container = container.build();

        let vec = container.resolve::<Cache>().unwrap();
        vec.lock().unwrap().push(1);

        let mut req = Request::get("/").body(()).unwrap();
        req.extensions_mut().insert(container);
        let (parts, _) = req.into_parts();
        let req = HttpRequest::from_parts(parts, HttpBody::empty());

        // Act
        let dc = Dc::<Cache>::from_request(&req).unwrap();

        // Assert
        assert_eq!(dc.lock().unwrap().first().cloned(), Some(1));
    }

    #[test]
    fn it_extracts_from_parts() {
        let mut container = ContainerBuilder::new();
        container.register_singleton::<Cache>(Cache::default());
        let container = container.build();

        let vec = container.resolve::<Cache>().unwrap();
        vec.lock().unwrap().push(1);

        let mut req = Request::get("/").body(()).unwrap();
        req.extensions_mut().insert(container);
        let (parts, _) = req.into_parts();

        let dc = Dc::<Cache>::from_parts(&parts).unwrap();

        assert_eq!(dc.lock().unwrap().first().cloned(), Some(1));
    }

    #[test]
    fn it_resolves_shared_references() {
        let mut container = ContainerBuilder::new();
        container.register_singleton::<Cache>(Arc::new(Mutex::new(Vec::new())));
        let container = container.build();

        let mut extensions = Extensions::new();
        extensions.insert(container);

        let dc1 = Dc::<Cache>::try_from(&extensions).unwrap();
        dc1.lock().unwrap().push(1);

        let dc2 = Dc::<Cache>::try_from(&extensions).unwrap();
        dc2.lock().unwrap().push(2);

        let final_vec = dc1.lock().unwrap();
        assert_eq!(final_vec.len(), 2);
        assert_eq!(final_vec[0], 1);
        assert_eq!(final_vec[1], 2);
    }

}