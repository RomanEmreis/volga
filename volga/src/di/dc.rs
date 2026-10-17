//! Extractors for Dependency Injection

use super::{Container, Dependencies, FromContainer, Inject, error::Error as DiError};
use crate::{
    HttpRequest,
    error::Error,
    http::endpoints::args::{FromPayload, FromRequestParts, FromRequestRef, Payload, Source},
};
use futures_util::future::{Ready, ready};
use hyper::http::{Extensions, request::Parts};

use std::{ops::Deref, sync::Arc};

/// `Dc` stands for Dependency Container.
///
/// This struct wraps an injectable type `T` that is **shared** through an [`Arc`]: a
/// singleton with every request the app serves, a scoped service with everything that
/// resolves it during one request. So `Dc<T>` gives `&T` and never `&mut T`.
///
/// # Changing a dependency
/// State that handlers change lives inside `T`, behind interior mutability, and `T`'s
/// methods change it through `&self`:
///
/// - a [`Mutex`](std::sync::Mutex) or an [`RwLock`](std::sync::RwLock) around the data,
///   an `RwLock` where reads outnumber writes;
/// - an atomic, such as [`AtomicU64`](std::sync::atomic::AtomicU64), for a counter or a flag;
/// - a [`tokio::sync::Mutex`] only where the guard is held across an `.await`. A
///   `std::sync` guard is dropped before one.
///
/// `T` needs no [`Arc`] of its own around that state, since the container already shares
/// the instance. It does when `T` is [`Clone`] and is also resolved by value - by
/// [`Container::resolve`], [`Dc::cloned`] or into a field of another service - so that every
/// clone reaches the same state.
///
/// A handler that wants a copy of its own to change takes [`Dc::cloned`]. The registered
/// instance stays as it was.
///
/// # Example
/// ```no_run
/// use volga::{App, di::Dc, ok, not_found};
/// use std::{
///     collections::HashMap,
///     sync::{RwLock, atomic::{AtomicU64, Ordering}}
/// };
///
/// #[derive(Default)]
/// struct InMemoryCache {
///     users: RwLock<HashMap<String, String>>,
///     hits: AtomicU64,
/// }
///
///# #[tokio::main]
///# async fn main() -> std::io::Result<()> {
/// let mut app = App::new();
///
/// app.add_singleton(InMemoryCache::default());
///
/// app.map_get("/user/{id}", |id: String, cache: Dc<InMemoryCache>| async move {
///     cache.hits.fetch_add(1, Ordering::Relaxed);
///     let users = cache.users.read().unwrap();
///     match users.get(&id) {
///         Some(user) => ok!(user),
///         None => not_found!()
///     }
/// });
///
/// app.map_put("/user/{id}/{name}", |id: String, name: String, cache: Dc<InMemoryCache>| async move {
///     cache.users.write().unwrap().insert(id, name);
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
    /// Equivalent to calling [`Clone::clone`] on the inner `T`. The copy is the caller's
    /// own: changing it leaves the registered instance as it was, apart from whatever
    /// state `T` shares between its clones, such as an [`Arc`] field.
    #[inline]
    pub fn cloned(&self) -> T {
        self.0.as_ref().clone()
    }
}

impl<T: Send + Sync + 'static> TryFrom<&Extensions> for Dc<T> {
    type Error = Error;

    #[inline]
    fn try_from(extensions: &Extensions) -> Result<Self, Self::Error> {
        // Borrow the container rather than clone it. This runs once per `Dc<T>` argument
        // of every handler, and the container is only read here.
        let container: &Container = extensions.try_into()?;
        Self::from_container(container)
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
        container.resolve_shared::<T>().map_err(Into::into).map(Dc)
    }
}

impl<T: Send + Sync + 'static> FromPayload for Dc<T> {
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

impl<T: Send + Sync + 'static> Inject for Dc<T> {
    #[inline]
    fn inject(container: &Container) -> Result<Self, DiError> {
        container.resolve_shared::<T>().map(Dc)
    }

    #[inline]
    fn dependencies(deps: &mut Dependencies) {
        deps.add::<T>();
    }
}

#[cfg(test)]
mod tests {
    use super::{Dc, DiError};
    use crate::di::{Container, ContainerBuilder, Inject};
    use crate::http::endpoints::args::{FromPayload, FromRequestParts, FromRequestRef, Payload};
    use crate::{HttpBody, HttpRequest};
    use hyper::Request;
    use hyper::http::Extensions;
    use std::sync::{Arc, Mutex};

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

        let dc = Dc::<Cache>::from_payload(Payload::Parts(&parts))
            .await
            .unwrap();
        dc.lock().unwrap().push(2);

        let dc = Dc::<Cache>::from_payload(Payload::Parts(&parts))
            .await
            .unwrap();
        dc.lock().unwrap().push(3);

        let dc = Dc::<Cache>::from_payload(Payload::Parts(&parts))
            .await
            .unwrap();
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

        let point = Dc::<Point>::inject(&container).unwrap().into_inner();

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

        let point = Dc::<Point>::inject(&container).unwrap().into_inner();

        assert_eq!(point.0.0, 1);
        assert_eq!(point.1.0, 2);
    }

    #[test]
    fn it_clones_inner_value() {
        let mut container = ContainerBuilder::new();
        container.register_singleton(X(1));

        let container = container.build();

        let x = Dc::<X>::inject(&container).unwrap();

        let mut copy = x.cloned();
        copy.0 = 2;

        assert_ne!(copy.0, x.0.0);
    }
}
