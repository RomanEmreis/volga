//! Extractors for Dependency Injection

use super::{Container, Inject, FromContainer, error::Error as DiError};
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

/// `Dc` stands for Dependency Container.
/// 
/// This struct wraps an injectable type `T` that is **shared** between all handlers
/// through an [`Arc`].  
///
/// # Example
/// ```no_run
/// use volga::{App, di::Dc, ok, not_found};
/// use std::{
///     collections::HashMap,
///     sync::{Arc, Mutex}
/// };
///
/// #[derive(Default)]
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
#[derive(Debug, Clone)]
pub struct Dc<T: Send + Sync>(Arc<T>);

impl<T: Send + Sync> Deref for Dc<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T: Clone + Send + Sync> DerefMut for Dc<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut T {
        Arc::make_mut(&mut self.0)
    }
}

impl<T: Send + Sync> Dc<T> {
    /// Unwraps the inner [`Arc`]
    #[inline]
    pub fn into_inner(self) -> Arc<T> {
        self.0
    }
}

impl<T: Send + Sync + Clone> Dc<T> {
    /// Clones and returns the inner `T`.
    /// 
    /// Equivalent to calling [`Clone::clone`] on the inner `T`.
    #[inline]
    pub fn cloned(&self) -> T {
        self.0.as_ref().clone()
    }
}

impl<T: Send + Sync + 'static> TryFrom<&Extensions> for Dc<T> {
    type Error = Error;
    
    #[inline]
    fn try_from(extensions: &Extensions) -> Result<Self, Self::Error> {
        let container = Container::try_from(extensions)?;
        Self::from_container(&container)
    }
}

impl<T: Send + Sync + 'static> FromRequestRef for Dc<T> {
    #[inline]
    fn from_request(req: &HttpRequest) -> Result<Self, Error> {
        req.extensions().try_into()
    }    
}

impl<T: Send + Sync + 'static> FromRequestParts for Dc<T> {
    #[inline]
    fn from_parts(parts: &Parts) -> Result<Self, Error> {
        let ext = &parts.extensions;
        ext.try_into()
    }
}

impl<T: Send + Sync + 'static> FromContainer for Dc<T> {
    #[inline]
    fn from_container(container: &Container) -> Result<Self, Error> {
        container
            .resolve_shared::<T>()
            .map_err(Into::into)
            .map(Dc)
    }
}

impl<T: Send + Sync + 'static> FromPayload for Dc<T> {
    type Future = Ready<Result<Self, Error>>;

    const SOURCE: Source = Source::Parts;
    
    #[inline]
    fn from_payload(payload: Payload<'_>) -> Self::Future {
        let Payload::Parts(parts) = payload else { unreachable!() };
        ready(Self::from_parts(parts))
    }
}

impl<T: Send + Sync + 'static> Inject for Dc<T> {
    #[inline]
    fn inject(container: &Container) -> Result<Self, DiError> {
        container
            .resolve_shared::<T>()
            .map(Dc)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use hyper::http::Extensions;
    use hyper::Request;
    use super::{Dc, DiError};
    use crate::di::{ContainerBuilder, Inject, Container};
    use crate::http::endpoints::args::{FromPayload, FromRequestRef, FromRequestParts, Payload};
    use crate::{HttpBody, HttpRequest};

    type Cache = Arc<Mutex<Vec<i32>>>;

    #[derive(Debug, Clone, Copy)]
    struct X(i32);

    #[derive(Debug, Clone, Copy)]
    struct Y(i32);

    #[derive(Debug, Clone, Copy)]
    struct Point(X, Y);

    impl Inject for X {
        fn inject(container: &Container) -> Result<Self, DiError> {
            container.resolve()
        }
    }

    impl Inject for Y {
        fn inject(container: &Container) -> Result<Self, DiError> {
            container.resolve()
        }
    }
    
    #[tokio::test]
    async fn it_reads_from_payload() {
        let mut container = ContainerBuilder::new();
        
        container.register_scoped_default::<Cache>();
        
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

    #[test]
    fn it_resolves_by_injection() {
        let mut container = ContainerBuilder::new();
        container.register_transient_factory(|| X(1));
        container.register_transient_factory(|| Y(2));
        container.register_transient_factory(|x: X, y: Y| Ok(Point(x, y)));

        let container = container.build();

        let point = Dc::<Point>::inject(&container)
            .unwrap()
            .into_inner();
        
        assert_eq!(point.0.0, 1);
        assert_eq!(point.1.0, 2);
    }

    #[test]
    fn it_resolves_from_container() {
        let mut container = ContainerBuilder::new();
        container.register_transient_factory(|| X(1));
        container.register_transient_factory(|| Y(2));
        container.register_transient_factory(|c: Container| {
            let x: X = c.resolve()?;
            let y: Y = c.resolve()?;
            Ok(Point(x, y))
        });

        let container = container.build();

        let point = Dc::<Point>::inject(&container)
            .unwrap()
            .into_inner();
        
        assert_eq!(point.0.0, 1);
        assert_eq!(point.1.0, 2);
    }

    #[test]
    fn it_clones_inner_value() {
        let mut container = ContainerBuilder::new();
        container.register_singleton(X(1));

        let container = container.build();

        let x = Dc::<X>::inject(&container)
            .unwrap();
        
        let mut copy = x.cloned();
        copy.0 = 2;

        assert_ne!(copy.0, x.0.0);
    }
}