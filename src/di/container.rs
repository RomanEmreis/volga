use super::{Inject, DiError};
use crate::error::Error;
use futures_util::TryFutureExt;
use tokio::sync::OnceCell;
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

#[derive(Clone)]
pub(crate) enum ServiceEntry {
    Singleton(ArcService),
    Scoped(OnceCell<ArcService>),
    Transient,
}

impl ServiceEntry {
    #[inline]
    fn as_scope(&self) -> Self {
        match self {
            ServiceEntry::Singleton(service) => ServiceEntry::Singleton(service.clone()),
            ServiceEntry::Scoped(_) => ServiceEntry::Scoped(OnceCell::new()),
            ServiceEntry::Transient => ServiceEntry::Transient,
        }
    }
}

type ServiceMap = HashMap<TypeId, ServiceEntry, BuildHasherDefault<TypeIdHasher>>;

#[derive(Default)]
struct TypeIdHasher(u64);

impl Hasher for TypeIdHasher {
    fn write(&mut self, _: &[u8]) {
        unreachable!("TypeId calls write_u64");
    }

    #[inline]
    fn write_u64(&mut self, id: u64) {
        self.0 = id;
    }

    #[inline]
    fn finish(&self) -> u64 {
        self.0
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
            services: self.services
        }
    }

    /// Register a singleton service
    pub fn register_singleton<T: Inject + 'static>(&mut self, instance: T) {
        let entry = ServiceEntry::Singleton(Arc::new(instance));
        self.services.insert(TypeId::of::<T>(), entry);
    }

    /// Register a scoped service
    pub fn register_scoped<T: Inject + 'static>(&mut self) {
        let entry = ServiceEntry::Scoped(OnceCell::new());
        self.services.insert(TypeId::of::<T>(), entry);
    }

    /// Register a transient service
    pub fn register_transient<T: Inject + 'static>(&mut self) {
        let entry = ServiceEntry::Transient;
        self.services.insert(TypeId::of::<T>(), entry);
    }
}

/// Represents a DI container
pub struct Container {
    services: ServiceMap
}

impl Clone for Container {
    #[inline]
    fn clone(&self) -> Self {
        let services = self.services.iter()
            .map(|(key, value)| (*key, value.clone()))
            .collect();
        Self { services }
    }
}

impl Container {
    /// Creates a new container where Scoped services are not created yet
    #[inline]
    pub fn create_scope(&self) -> Self {
        let services = self.services.iter()
            .map(|(key, value)| (*key, value.as_scope()))
            .collect();
        Self { services }
    }

    /// Resolve a service
    pub async fn resolve<T: Inject + 'static>(&mut self) -> Result<T, Error> {
        match self.get_service_entry::<T>()? {
            ServiceEntry::Transient => T::inject(self).await,
            ServiceEntry::Singleton(instance) => Self::resolve_internal(instance).cloned(),
            ServiceEntry::Scoped(cell) => {
                let instance = cell
                    .get_or_try_init(|| async {
                        T::inject(&mut self.clone())
                            .map_ok(|scoped| Arc::new(scoped) as ArcService)
                            .await
                    })
                    .await?;
                Self::resolve_internal(instance).cloned()
            }
        }
    }

    /// Resolve a service as ref
    pub async fn resolve_ref<T: Inject + 'static>(&mut self) -> Result<&T, Error> {
        match self.get_service_entry::<T>()? {
            ServiceEntry::Transient => Err(Error::server_error(
                "cannot resolve a `Transient` service as ref, use `resolve::<T>()` or `Dc<T>` instead",
            )),
            ServiceEntry::Singleton(instance) => Self::resolve_internal(instance),
            ServiceEntry::Scoped(cell) => {
                let instance = cell
                    .get_or_try_init(|| async {
                        T::inject(&mut self.clone())
                            .map_ok(|scoped| Arc::new(scoped) as ArcService)
                            .await
                    })
                    .await?;
                Self::resolve_internal(instance)
            }
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
    fn resolve_internal<T: Inject + 'static>(instance: &ArcService) -> Result<&T, Error> {
        (**instance)
            .downcast_ref::<T>()
            .ok_or(DiError::resolve_error(std::any::type_name::<T>()))
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};
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
        async fn inject(container: &mut Container) -> Result<Self, Error> {
            Ok(Self { inner: container.resolve().await? })
        }
    }

    #[tokio::test]
    async fn it_registers_singleton() {
        let mut container = ContainerBuilder::new();
        container.register_singleton(InMemoryCache::default());

        let mut container = container.build();

        let cache = container.resolve::<InMemoryCache>().await.unwrap();
        cache.set("key", "value");

        let cache = container.resolve::<InMemoryCache>().await.unwrap();
        let key = cache.get("key").unwrap();

        assert_eq!(key, "value");
    }

    #[tokio::test]
    async fn it_registers_singleton_and_resolves_as_ref() {
        let mut container = ContainerBuilder::new();
        container.register_singleton(InMemoryCache::default());

        let mut container = container.build();

        let cache = container.resolve_ref::<InMemoryCache>().await.unwrap();
        cache.set("key", "value");

        let cache = container.resolve_ref::<InMemoryCache>().await.unwrap();
        let key = cache.get("key").unwrap();

        assert_eq!(key, "value");
    }

    #[tokio::test]
    async fn it_registers_transient() {
        let mut container = ContainerBuilder::new();
        container.register_transient::<InMemoryCache>();

        let mut container = container.build();

        let cache = container.resolve::<InMemoryCache>().await.unwrap();
        cache.set("key", "value");

        let cache = container.resolve::<InMemoryCache>().await.unwrap();
        let key = cache.get("key");

        assert!(key.is_none());
    }

    #[tokio::test]
    async fn it_registers_scoped() {
        let mut container = ContainerBuilder::new();
        container.register_scoped::<InMemoryCache>();

        let mut container = container.build();

        // working in the initial scope
        let cache = container.resolve::<InMemoryCache>().await.unwrap();
        cache.set("key", "value 1");

        // create a new scope so new instance of InMemoryCache will be created
        {
            let mut scope = container.create_scope();
            let cache = scope.resolve::<InMemoryCache>().await.unwrap();
            cache.set("key", "value 2");

            let cache = scope.resolve::<InMemoryCache>().await.unwrap();
            let key = cache.get("key").unwrap();

            assert_eq!(key, "value 2");
        }

        // create a new scope so new instance of InMemoryCache will be created
        {
            let mut scope = container.create_scope();
            let cache = scope.resolve::<InMemoryCache>().await.unwrap();
            let key = cache.get("key");

            assert!(key.is_none());
        }

        let key = cache.get("key").unwrap();

        assert_eq!(key, "value 1");
    }

    #[tokio::test]
    async fn it_registers_scoped_and_resolves_as_ref() {
        let mut container = ContainerBuilder::new();
        container.register_scoped::<InMemoryCache>();

        let mut container = container.build();

        // working in the initial scope
        let cache = container.resolve::<InMemoryCache>().await.unwrap();
        cache.set("key", "value 1");

        // create a new scope so new instance of InMemoryCache will be created
        {
            let mut scope = container.create_scope();
            let cache = scope.resolve_ref::<InMemoryCache>().await.unwrap();
            cache.set("key", "value 2");

            let cache = scope.resolve_ref::<InMemoryCache>().await.unwrap();
            let key = cache.get("key").unwrap();

            assert_eq!(key, "value 2");
        }

        // create a new scope so new instance of InMemoryCache will be created
        {
            let mut scope = container.create_scope();
            let cache = scope.resolve_ref::<InMemoryCache>().await.unwrap();
            let key = cache.get("key");

            assert!(key.is_none());
        }

        let key = cache.get("key").unwrap();

        assert_eq!(key, "value 1");
    }

    #[tokio::test]
    async fn it_resolves_inner_dependencies() {
        let mut container = ContainerBuilder::new();

        container.register_singleton(InMemoryCache::default());
        container.register_scoped::<CacheWrapper>();

        let mut container = container.build();

        {
            let mut scope = container.create_scope();
            let cache = scope.resolve::<CacheWrapper>().await.unwrap();
            cache.inner.set("key", "value 1");
        }

        let cache = container.resolve::<InMemoryCache>().await.unwrap();
        let key = cache.get("key").unwrap();

        assert_eq!(key, "value 1");
    }

    #[tokio::test]
    async fn it_resolves_inner_dependencies_as_ref() {
        let mut container = ContainerBuilder::new();

        container.register_singleton(InMemoryCache::default());
        container.register_scoped::<CacheWrapper>();

        let mut container = container.build();

        {
            let mut scope = container.create_scope();
            let cache = scope.resolve_ref::<CacheWrapper>().await.unwrap();
            cache.inner.set("key", "value 1");
        }

        let cache = container.resolve_ref::<InMemoryCache>().await.unwrap();
        let key = cache.get("key").unwrap();

        assert_eq!(key, "value 1");
    }

    #[tokio::test]
    async fn inner_scope_does_not_affect_outer() {
        let mut container = ContainerBuilder::new();

        container.register_scoped::<InMemoryCache>();
        container.register_scoped::<CacheWrapper>();

        let mut container = container.build();

        {
            let mut scope = container.create_scope();
            let cache = scope.resolve::<CacheWrapper>().await.unwrap();
            cache.inner.set("key", "value 1");

            let cache = scope.resolve::<CacheWrapper>().await.unwrap();
            cache.inner.set("key", "value 2");
        }

        let cache = container.resolve::<InMemoryCache>().await.unwrap();
        let key = cache.get("key");

        assert!(key.is_none())
    }

    #[tokio::test]
    async fn inner_scope_does_not_affect_outer_with_ref() {
        let mut container = ContainerBuilder::new();

        container.register_scoped::<InMemoryCache>();
        container.register_scoped::<CacheWrapper>();

        let mut container = container.build();

        {
            let mut scope = container.create_scope();
            let cache = scope.resolve_ref::<CacheWrapper>().await.unwrap();
            cache.inner.set("key", "value 1");

            let cache = scope.resolve_ref::<CacheWrapper>().await.unwrap();
            cache.inner.set("key", "value 2");
        }

        let cache = container.resolve_ref::<InMemoryCache>().await.unwrap();
        let key = cache.get("key");

        assert!(key.is_none())
    }

    #[tokio::test]
    async fn it_resolves_inner_scoped_dependencies() {
        let mut container = ContainerBuilder::new();

        container.register_scoped::<InMemoryCache>();
        container.register_scoped::<CacheWrapper>();

        let container = container.build();

        let mut scope = container.create_scope();
        let cache = scope.resolve::<CacheWrapper>().await.unwrap();
        cache.inner.set("key1", "value 1");

        let cache = scope.resolve::<CacheWrapper>().await.unwrap();
        cache.inner.set("key2", "value 2");

        let cache = scope.resolve::<CacheWrapper>().await.unwrap();

        assert_eq!(cache.inner.get("key1").unwrap(), "value 1");
        assert_eq!(cache.inner.get("key2").unwrap(), "value 2");
    }

    #[tokio::test]
    async fn it_resolves_inner_scoped_dependencies_as_ref() {
        let mut container = ContainerBuilder::new();

        container.register_scoped::<InMemoryCache>();
        container.register_scoped::<CacheWrapper>();

        let container = container.build();

        let mut scope = container.create_scope();
        let cache = scope.resolve_ref::<CacheWrapper>().await.unwrap();
        cache.inner.set("key1", "value 1");

        let cache = scope.resolve_ref::<CacheWrapper>().await.unwrap();
        cache.inner.set("key2", "value 2");

        let cache = scope.resolve_ref::<CacheWrapper>().await.unwrap();

        assert_eq!(cache.inner.get("key1").unwrap(), "value 1");
        assert_eq!(cache.inner.get("key2").unwrap(), "value 2");
    }
}