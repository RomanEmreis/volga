﻿//! Extractors for Dependency Injection

use super::{Container, Inject};
use futures_util::{pin_mut, ready};
use pin_project_lite::pin_project;

use crate::{
    error::Error, 
    http::endpoints::args::{FromPayload, Payload, Source}
};

use std::{
    ops::{Deref, DerefMut},
    task::{Context, Poll},
    marker::PhantomData,
    future::Future,
    pin::Pin,
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

pin_project! {
    /// A future that resolves a dependency from DI container.
    pub struct ExtractDependencyFut<T> {
        #[pin]
        container: Container,
        _marker: PhantomData<T>
    }
}

impl<T: Inject + 'static> Future for ExtractDependencyFut<T> {
    type Output = Result<Dc<T>, Error>;
    
    #[inline]
    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let this = self.project();
        let fut = this.container.resolve_shared::<T>();
        pin_mut!(fut);
        let result = ready!(fut.poll(cx));
        Poll::Ready(result.map(Dc))
    }
}

impl<T: Inject + 'static> FromPayload for Dc<T> {
    type Future = ExtractDependencyFut<T>;

    fn from_payload(payload: Payload) -> Self::Future {
        let Payload::Parts(parts) = payload else { unreachable!() };
        let container = Container::try_from(parts)
            .expect("DI Container must be provided");
        ExtractDependencyFut { container, _marker: PhantomData }
    }

    fn source() -> Source {
        Source::Parts
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};
    use hyper::Request;
    use super::Dc;
    use crate::di::ContainerBuilder;
    use crate::http::endpoints::args::{FromPayload, Payload};
    
    type Cache = Arc<Mutex<Vec<i32>>>;
    
    #[tokio::test]
    async fn it_reads_from_payload() {
        let mut container = ContainerBuilder::new();
        
        container.register_scoped::<Cache>();
        
        let container = container.build();
        
        let scope = container.create_scope();
        let vec = scope.resolve::<Cache>().await.unwrap();
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
}