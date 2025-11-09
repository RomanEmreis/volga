//! Dependency Injection container and tools

use crate::{Inject, error::Error};
use http::{Extensions, request::Parts};
use std::{
    any::{Any, TypeId},
    collections::HashMap,
    fmt::Debug,
    hash::{BuildHasherDefault, Hasher},
    sync::{Arc, OnceLock}
};

/// A trait that defines how to extract the `Self` from DI container
pub trait FromContainer: Sized + Send + Sync {
    /// Extracts `Self` from DI container
    fn from_container(container: &Container) -> Result<Self, Error>;
}

impl FromContainer for Container {
    #[inline]
    fn from_container(container: &Container) -> Result<Self, Error> {
        Ok(container.clone())
    }
}

impl FromContainer for () {
    #[inline]
    fn from_container(_: &Container) -> Result<Self, Error> {
        Ok(())
    }
}

/// A trait that describes a generic factory function 
/// that can resolve objects registered in DI container
pub trait GenericFactory<Args>: Clone + Send + Sync + 'static {
    /// A type of object that will be resolved
    type Output;
    
    /// Calls a generic function and returns either resolved object or error
    fn call(&self, args: Args) -> Result<Self::Output, Error>;
}

impl<F, R> GenericFactory<()> for F
where
    F: Fn() -> R + Clone + Send + Sync + 'static
{
    type Output = R;
    
    #[inline]
    fn call(&self, _: ()) -> Result<Self::Output, Error> {
        Ok(self())
    }
}

macro_rules! define_generic_factory ({ $($param:ident)* } => {
    impl<F, R, $($param,)*> GenericFactory<($($param,)*)> for F
    where
        F: Fn($($param),*) -> Result<R, Error> + Clone + Send + Sync + 'static
    {
        type Output = R;

        #[inline]
        #[allow(non_snake_case)]
        fn call(&self, ($($param,)*): ($($param,)*)) -> Result<Self::Output, Error> {
            (self)($($param,)*)
        }
    }    
});

define_generic_factory! { T1 }
define_generic_factory! { T1 T2 }
define_generic_factory! { T1 T2 T3 }

macro_rules! define_generic_from_container {
    ($($T: ident),*) => {
        impl<$($T: FromContainer),+> FromContainer for ($($T,)+) {
            #[inline]
            #[allow(non_snake_case)]
            fn from_container(container: &Container) -> Result<Self, Error>{
                let tuple = (
                    $(
                    $T::from_container(container)?,
                    )*    
                );
                Ok(tuple)
            }
        }
    }
}

define_generic_from_container! { T1 }
define_generic_from_container! { T1, T2 }
define_generic_from_container! { T1, T2, T3 }

#[inline]
fn make_resolver_fn<T, F, Args>(resolver: F) -> ResolverFn
where
    T: Send + Sync + 'static,
    F: GenericFactory<Args, Output = T>,
    Args: FromContainer
{
    Arc::new(move |c: &Container| -> Result<ArcService, Error> {
        let args = Args::from_container(c)?;
        resolver.call(args).map(|t| Arc::new(t) as ArcService)
    })
}

type ResolverFn = Arc<
    dyn Fn(&Container) -> Result<ArcService, Error> 
    + Send 
    + Sync
>;

type ArcService = Arc<
    dyn Any
    + Send
    + Sync
>;

pub(crate) enum ServiceEntry {
    Singleton(ArcService),
    Scoped(OnceLock<Result<ArcService, Error>>, ResolverFn),
    Transient(ResolverFn),
}

impl Debug for ServiceEntry {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ServiceEntry(..)")
    }
}

impl ServiceEntry {
    #[inline]
    fn as_scope(&self) -> Self {
        match self {
            ServiceEntry::Singleton(service) => ServiceEntry::Singleton(service.clone()),
            ServiceEntry::Scoped(_, r) => ServiceEntry::Scoped(OnceLock::new(), r.clone()),
            ServiceEntry::Transient(r) => ServiceEntry::Transient(r.clone()),
        }
    }
}

/// Inner HashMap of dependencies
type ServiceMap = HashMap<
    TypeId, 
    ServiceEntry, 
    BuildHasherDefault<TypeIdHasher>
>;

#[derive(Default)]
struct TypeIdHasher(u64);

impl Hasher for TypeIdHasher {
    #[inline]
    fn finish(&self) -> u64 {
        self.0
    }

    #[cold]
    fn write(&mut self, _: &[u8]) {
        unreachable!("TypeId calls write_u64");
    }

    #[inline]
    fn write_u64(&mut self, id: u64) {
        self.0 = id;
    }
}

/// Represents a DI container builder,
/// that is able to add/register dependencies with a specific lifetimes.
#[derive(Debug)]
pub struct ContainerBuilder {
    /// Configurable HashMap of dependencies
    services: ServiceMap
}

impl Default for ContainerBuilder {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl ContainerBuilder {
    /// Creates a new DI container builder
    #[inline]
    pub fn new() -> Self {
        Self { services: ServiceMap::default() }
    }

    /// Build a DI container
    #[inline]
    pub fn build(self) -> Container {
        Container {
            services: Arc::new(self.services),
        }
    }

    /// Register a singleton service
    pub fn register_singleton<T: Send + Sync + 'static>(&mut self, instance: T) {
        let entry = ServiceEntry::Singleton(Arc::new(instance));
        self.services.insert(TypeId::of::<T>(), entry);
    }

    /// Register a scoped service
    pub fn register_scoped_factory<T, F, Args>(&mut self, factory: F)
    where
        T: Send + Sync + 'static,
        F: GenericFactory<Args, Output = T>,
        Args: FromContainer
    {
        let entry = ServiceEntry::Scoped(OnceLock::new(), make_resolver_fn(factory));
        self.services.insert(TypeId::of::<T>(), entry);
    }

    /// Register a transient service that required to be resolved as [`Default`]
    pub fn register_scoped_default<T>(&mut self)
    where
        T: Default + Send + Sync + 'static
    {
        self.register_scoped_factory(T::default);
    }

    /// Register a transient service that required to be resolved as [`Inject`]
    pub fn register_scoped<T: Inject + 'static>(&mut self) {
        self.register_scoped_factory(|c: Container| T::inject(&c));
    }
    
    /// Register a transient service
    pub fn register_transient_factory<T, F, Args>(&mut self, factory: F)
    where
        T: Send + Sync + 'static,
        F: GenericFactory<Args, Output = T>,
        Args: FromContainer
    {
        let entry = ServiceEntry::Transient(make_resolver_fn(factory));
        self.services.insert(TypeId::of::<T>(), entry);
    }

    /// Register a transient service that required to be resolved as [`Default`]
    pub fn register_transient_default<T>(&mut self)
    where
        T: Default + Send + Sync + 'static
    {
        self.register_transient_factory(T::default);
    }

    /// Register a transient service that required to be resolved as [`Inject`]
    pub fn register_transient<T: Inject + 'static>(&mut self) {
        self.register_transient_factory(|c: Container| T::inject(&c));
    }
}

/// Represents a DI container, that is able to resolve generic dependencies
#[derive(Debug, Clone)]
pub struct Container {
    /// Read-only HashMap of dependencies
    services: Arc<ServiceMap>
}

impl Container {
    /// Creates a new child dependency-injection scope that inherits all service
    /// registrations from its parent:
    ///
    /// - **Singleton** services are shared: the child scope reuses the parent’s
    ///   singleton instances.
    /// - **Scoped** services are isolated: they are not instantiated upfront and
    ///   will be lazily created the first time they are resolved within this scope.
    /// - **Transient** services preserve their lifetime semantics: each resolution
    ///   returns a newly constructed instance.
    ///
    /// This method is typically used to create request-level or operation-level
    /// scopes when resolving services that should not live for the entire lifetime
    /// of the root container.
    #[inline]
    pub fn create_scope(&self) -> Self {
        let services = self.services.iter()
            .map(|(key, value)| (*key, value.as_scope()))
            .collect::<HashMap<_, _, _>>();
        Self { services: Arc::new(services) }
    }

    /// Resolves a service and returns a cloned instance. 
    /// `T` must implement [`Clone`] otherwise use [`resolve_shared`] method 
    /// that returns a shared pointer.
    #[inline]
    pub fn resolve<T: Send + Sync + Clone + 'static>(&self) -> Result<T, Error> {
        self.resolve_shared::<T>()
            .map(|s| s.as_ref().clone())
    }

    /// Resolves a service and returns a shared pointer
    #[inline]
    pub fn resolve_shared<T: Send + Sync + 'static>(&self) -> Result<Arc<T>, Error> {
        match self.get_service_entry::<T>()? {
            ServiceEntry::Transient(r) => r(self).and_then(|s| Self::resolve_internal(&s)),
            ServiceEntry::Scoped(cell, r) => self.resolve_scoped(cell, r),
            ServiceEntry::Singleton(instance) => Self::resolve_internal(instance)
        }
    }

    /// Fetch the service entry or return an error if not registered.
    #[inline]
    fn get_service_entry<T: Send + Sync + 'static>(&self) -> Result<&ServiceEntry, Error> {
        let type_id = TypeId::of::<T>();
        self.services
            .get(&type_id)
            .ok_or_else(|| Error::NotRegistered(std::any::type_name::<T>()))
    }

    #[inline]
    fn resolve_scoped<T: Send + Sync + 'static>(
        &self, 
        cell: &OnceLock<Result<ArcService, Error>>,
        resolver_fn: &ResolverFn
    ) -> Result<Arc<T>, Error> {
        cell.get_or_init(|| resolver_fn(self))
            .as_ref()
            .map_err(|err| *err)
            .and_then(Self::resolve_internal)
    }

    #[inline]
    fn resolve_internal<T: Send + Sync + 'static>(instance: &ArcService) -> Result<Arc<T>, Error> {
        instance
            .clone()
            .downcast::<T>()
            .map_err(|_| Error::ResolveFailed(std::any::type_name::<T>()))
    }
}

impl<'a> TryFrom<&'a Extensions> for &'a Container {
    type Error = Error;

    #[inline]
    fn try_from(extensions: &'a Extensions) -> Result<Self, Self::Error> {
        extensions.get::<Container>()
            .ok_or(Error::ContainerMissing)
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
    use http::Request;
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
        container.register_transient_default::<InMemoryCache>();

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
        container.register_scoped_default::<InMemoryCache>();

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

        container.register_scoped_default::<InMemoryCache>();
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

        container.register_scoped_default::<InMemoryCache>();
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