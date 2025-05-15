//! Dependency Injection container and tools

use super::{Inject, DiError};
use crate::error::Error;
use hyper::http::{Extensions, request::Parts};
use std::sync::OnceLock;
use std::{
    any::{Any, TypeId},
    collections::HashMap,
    hash::{BuildHasherDefault, Hasher},
    sync::Arc
};

type ArcService = Arc<
    dyn Any
    + Send
    + Sync
>;

pub(crate) enum ServiceEntry {
    Singleton(ArcService),
    Scoped(OnceLock<Result<ArcService, Error>>),
    Transient,
}

impl ServiceEntry {
    #[inline]
    fn as_scope(&self) -> Self {
        match self {
            ServiceEntry::Singleton(service) => ServiceEntry::Singleton(service.clone()),
            ServiceEntry::Scoped(_) => ServiceEntry::Scoped(OnceLock::new()),
            ServiceEntry::Transient => ServiceEntry::Transient,
        }
    }
}

type ServiceMap = HashMap<TypeId, ServiceEntry, BuildHasherDefault<TypeIdHasher>>;

#[derive(Default)]
struct TypeIdHasher(u64);

impl Hasher for TypeIdHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }

    fn write(&mut self, _: &[u8]) {
        unreachable!("TypeId calls write_u64");
    }

    #[inline]
    fn write_u64(&mut self, id: u64) {
        self.0 = id;
    }
}

/// Represents a DI container builder
pub struct ContainerBuilder {
    services: ServiceMap
}

impl Default for ContainerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl ContainerBuilder {
    /// Create a new DI container builder
    pub fn new() -> Self {
        Self { services: ServiceMap::default() }
    }

    /// Build a DI container
    pub fn build(self) -> Container {
        Container {
            services: Arc::new(self.services),
        }
    }

    /// Register a singleton service
    pub fn register_singleton<T: Inject + 'static>(&mut self, instance: T) {
        let entry = ServiceEntry::Singleton(Arc::new(instance));
        self.services.insert(TypeId::of::<T>(), entry);
    }

    /// Register a scoped service
    pub fn register_scoped<T: Inject + 'static>(&mut self) {
        let entry = ServiceEntry::Scoped(OnceLock::new());
        self.services.insert(TypeId::of::<T>(), entry);
    }

    /// Register a transient service
    pub fn register_transient<T: Inject + 'static>(&mut self) {
        let entry = ServiceEntry::Transient;
        self.services.insert(TypeId::of::<T>(), entry);
    }
}

/// Represents a DI container
#[derive(Clone)]
pub struct Container {
    services: Arc<ServiceMap>
}

impl Container {
    /// Creates a new container where Scoped services are not created yet
    #[inline]
    pub fn create_scope(&self) -> Self {
        let services = self.services.iter()
            .map(|(key, value)| (*key, value.as_scope()))
            .collect::<HashMap<_, _, _>>();
        Self { services: Arc::new(services) }
    }

    /// Resolves a service and returns a cloned instance. 
    /// `T` must implement [`Clone`] otherwise use [`resolve_shared`] method 
    /// that returns a shared pointer
    #[inline]
    pub fn resolve<T: Inject + Clone + 'static>(&self) -> Result<T, Error> {
        self.resolve_shared::<T>()
            .map(|s| s.as_ref().clone())
    }

    /// Resolves a service and returns a shared pointer
    #[inline]
    pub fn resolve_shared<T: Inject + 'static>(&self) -> Result<Arc<T>, Error> {
        match self.get_service_entry::<T>()? {
            ServiceEntry::Transient => T::inject(self).map(Arc::new),
            ServiceEntry::Scoped(cell) => self.resolve_scoped(cell),
            ServiceEntry::Singleton(instance) => Self::resolve_internal(instance)
        }
    }

    /// Fetch the service entry or return an error if not registered.
    #[inline]
    fn get_service_entry<T: Inject + 'static>(&self) -> Result<&ServiceEntry, Error> {
        let type_id = TypeId::of::<T>();
        self.services
            .get(&type_id)
            .ok_or_else(|| DiError::service_not_registered(std::any::type_name::<T>()))
    }

    #[inline]
    fn resolve_scoped<T: Inject + 'static>(&self, cell: &OnceLock<Result<ArcService, Error>>) -> Result<Arc<T>, Error> {
        let instance = cell.get_or_init(|| 
            T::inject(self).map(|scoped| Arc::new(scoped) as ArcService)
        );
        instance
            .as_ref()
            .map_err(|err| Error::server_error(err.to_string()))
            .and_then(Self::resolve_internal)
    }

    #[inline]
    fn resolve_internal<T: Inject + 'static>(instance: &ArcService) -> Result<Arc<T>, Error> {
        instance
            .clone()
            .downcast::<T>()
            .map_err(|_| DiError::resolve_error(std::any::type_name::<T>()))
    }
}

impl<'a> TryFrom<&'a Extensions> for &'a Container {
    type Error = Error;

    #[inline]
    fn try_from(extensions: &'a Extensions) -> Result<Self, Self::Error> {
        extensions.get::<Container>()
            .ok_or(DiError::container_missing())
    }
}

impl TryFrom<&Extensions> for Container {
    type Error = Error;
    
    #[inline]
    fn try_from(extensions: &Extensions) -> Result<Self, Self::Error> {
        let res: Result<&Container, Error> = extensions.try_into();
        res.cloned()
    }
}

impl TryFrom<&Parts> for Container {
    type Error = Error;

    #[inline]
    fn try_from(parts: &Parts) -> Result<Self, Self::Error> {
        Container::try_from(&parts.extensions)
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
    use hyper::Request;
    use super::{Error, Container, ContainerBuilder, Inject};

    trait Cache: Send + Sync {
        fn get(&self, key: &str) -> Option<String>;
        fn set(&self, key: &str, value: &str);
    }

    #[derive(Clone, Default)]
    struct InMemoryCache {
        inner: Arc<Mutex<HashMap<String, String>>>
    }

    impl Cache for InMemoryCache {
        fn get(&self, key: &str) -> Option<String> {
            self.inner
                .lock()
                .unwrap()
                .get(key)
                .cloned()
        }

        fn set(&self, key: &str, value: &str) {
            self.inner
                .lock()
                .unwrap()
                .insert(key.to_string(), value.to_string());
        }
    }

    #[derive(Clone)]
    struct CacheWrapper {
        inner: InMemoryCache
    }

    impl Inject for CacheWrapper {
        fn inject(container: &Container) -> Result<Self, Error> {
            let inner = container.resolve::<InMemoryCache>()?;
            Ok(Self { inner })
        }
    }

    #[test]
    fn it_registers_singleton() {
        let mut container = ContainerBuilder::new();
        container.register_singleton(InMemoryCache::default());

        let container = container.build();

        let cache = container.resolve::<InMemoryCache>().unwrap();
        cache.set("key", "value");

        let cache = container.resolve::<InMemoryCache>().unwrap();
        let key = cache.get("key").unwrap();

        assert_eq!(key, "value");
    }

    #[test]
    fn it_registers_transient() {
        let mut container = ContainerBuilder::new();
        container.register_transient::<InMemoryCache>();

        let container = container.build();

        let cache = container.resolve::<InMemoryCache>().unwrap();
        cache.set("key", "value");

        let cache = container.resolve::<InMemoryCache>().unwrap();
        let key = cache.get("key");

        assert!(key.is_none());
    }

    #[test]
    fn it_registers_scoped() {
        let mut container = ContainerBuilder::new();
        container.register_scoped::<InMemoryCache>();

        let container = container.build();

        // working in the initial scope
        let cache = container.resolve::<InMemoryCache>().unwrap();
        cache.set("key", "value 1");

        // create a new scope so a new instance of InMemoryCache will be created
        {
            let scope = container.create_scope();
            let cache = scope.resolve::<InMemoryCache>().unwrap();
            cache.set("key", "value 2");

            let cache = scope.resolve::<InMemoryCache>().unwrap();
            let key = cache.get("key").unwrap();

            assert_eq!(key, "value 2");
        }

        // create a new scope so a new instance of InMemoryCache will be created
        {
            let scope = container.create_scope();
            let cache = scope.resolve::<InMemoryCache>().unwrap();
            let key = cache.get("key");

            assert!(key.is_none());
        }

        let key = cache.get("key").unwrap();

        assert_eq!(key, "value 1");
    }

    #[test]
    fn it_resolves_inner_dependencies() {
        let mut container = ContainerBuilder::new();

        container.register_singleton(InMemoryCache::default());
        container.register_scoped::<CacheWrapper>();

        let container = container.build();

        {
            let scope = container.create_scope();
            let cache = scope.resolve::<CacheWrapper>().unwrap();
            cache.inner.set("key", "value 1");
        }

        let cache = container.resolve::<InMemoryCache>().unwrap();
        let key = cache.get("key").unwrap();

        assert_eq!(key, "value 1");
    }

    #[test]
    fn inner_scope_does_not_affect_outer() {
        let mut container = ContainerBuilder::new();

        container.register_scoped::<InMemoryCache>();
        container.register_scoped::<CacheWrapper>();

        let container = container.build();

        {
            let scope = container.create_scope();
            let cache = scope.resolve::<CacheWrapper>().unwrap();
            cache.inner.set("key", "value 1");

            let cache = scope.resolve::<CacheWrapper>().unwrap();
            cache.inner.set("key", "value 2");
        }

        let cache = container.resolve::<InMemoryCache>().unwrap();
        let key = cache.get("key");

        assert!(key.is_none())
    }

    #[test]
    fn it_resolves_inner_scoped_dependencies() {
        let mut container = ContainerBuilder::new();

        container.register_scoped::<InMemoryCache>();
        container.register_scoped::<CacheWrapper>();

        let container = container.build();

        let scope = container.create_scope();
        let cache = scope.resolve::<CacheWrapper>().unwrap();
        cache.inner.set("key1", "value 1");

        let cache = scope.resolve::<CacheWrapper>().unwrap();
        cache.inner.set("key2", "value 2");

        let cache = scope.resolve::<CacheWrapper>().unwrap();

        assert_eq!(cache.inner.get("key1").unwrap(), "value 1");
        assert_eq!(cache.inner.get("key2").unwrap(), "value 2");
    }
    
    #[test]
    fn it_extracts_from_parts() {
        let mut container = ContainerBuilder::new();
        container.register_singleton(InMemoryCache::default());

        let container = container.build();
        
        let mut req = Request::get("/").body(()).unwrap();
        req.extensions_mut().insert(container.create_scope());

        let (parts, _) = req.into_parts();
        
        let container = Container::try_from(&parts);
        
        assert!(container.is_ok());
    }
    
    #[test]
    fn it_returns_error_when_resolve_unregistered() {
        let container = ContainerBuilder::new().build();

        let cache = container.resolve::<CacheWrapper>();
        
        assert!(cache.is_err());
    }

    #[test]
    fn it_returns_error_when_resolve_unregistered_from_scope() {
        let container = ContainerBuilder::new()
            .build()
            .create_scope();

        let cache = container.resolve::<CacheWrapper>();

        assert!(cache.is_err());
    }
}