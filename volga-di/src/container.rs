//! Dependency Injection container and tools

use crate::{
    Inject,
    error::{Error, Issue, ValidationError},
    inject::{Dependencies, Dependency},
};
use http::{Extensions, request::Parts};
use std::{
    any::{Any, TypeId},
    cell::RefCell,
    collections::HashMap,
    fmt::Debug,
    hash::{BuildHasherDefault, Hasher},
    sync::{Arc, OnceLock},
};

pub use factory::GenericFactory;

pub mod factory;

/// Helper function that creates a [`ResolverFn`] from regular functions
#[inline]
fn make_resolver_fn<T, F, Args>(resolver: F) -> ResolverFn
where
    T: Send + Sync + 'static,
    F: GenericFactory<Args, Output = T>,
    Args: Inject,
{
    Arc::new(move |c: &Container| -> Result<ArcService, Error> {
        let args = Args::inject(c)?;
        resolver.call(args).map(|t| Arc::new(t) as ArcService)
    })
}

/// Helper function that creates a [`ResolverFn`] for injectable types
#[inline]
fn make_inject_resolver_fn<T>() -> ResolverFn
where
    T: Inject + 'static,
{
    Arc::new(move |c: &Container| -> Result<ArcService, Error> {
        T::inject(c).map(|t| Arc::new(t) as ArcService)
    })
}

/// A dynamic resolver function for resolving objects
type ResolverFn = Arc<dyn Fn(&Container) -> Result<ArcService, Error> + Send + Sync>;

/// A dynamic wrapper for object in DI container
type ArcService = Arc<dyn Any + Send + Sync>;

/// A scope's own instance of a scoped service, filled the first time the service is
/// resolved in that scope
type ScopedCell = OnceLock<Result<ArcService, Error>>;

/// Represents a service registered with a specific lifetime in DI container
pub(crate) enum ServiceEntry {
    Singleton(ArcService),
    /// The index of this service's cell among a scope's cells, and how to build it.
    ///
    /// The index is handed out by [`ContainerBuilder::build`] and is a placeholder until
    /// then, which nothing can observe: only `build` turns a builder's entries into a
    /// container.
    Scoped(usize, ResolverFn),
    Transient(ResolverFn),
}

impl Debug for ServiceEntry {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ServiceEntry(..)")
    }
}

impl ServiceEntry {
    /// Creates a singleton [`ServiceEntry`]
    #[inline(always)]
    fn singleton<T: Send + Sync + 'static>(instance: T) -> Self {
        Self::Singleton(Arc::new(instance))
    }

    /// Creates a scoped [`ServiceEntry`], numbered later by [`ContainerBuilder::build`]
    #[inline(always)]
    fn scoped(resolver: ResolverFn) -> Self {
        Self::Scoped(0, resolver)
    }

    /// Creates a transient [`ServiceEntry`]
    #[inline(always)]
    fn transient(resolver: ResolverFn) -> Self {
        Self::Transient(resolver)
    }
}

/// Empty cells for `len` scoped services, in a single allocation
#[inline]
fn scoped_cells(len: usize) -> Arc<[ScopedCell]> {
    (0..len).map(|_| OnceLock::new()).collect()
}

thread_local! {
    /// The services this thread is constructing right now, outermost first, each with the
    /// registrations it is being built out of.
    ///
    /// Resolution is synchronous, so a construction that leads back to a service already
    /// on this stack is a dependency cycle - one that would otherwise re-enter a scoped
    /// cell in the middle of its initialization and deadlock, or recurse through
    /// transients until the stack overflows and takes the process with it.
    ///
    /// The registrations are half of the key because a service type says nothing on its
    /// own: a factory may build its service out of a container of its own, and two
    /// containers that share nothing but a type are not a loop. A container and every scope
    /// created from it do share their registrations, so a cycle through a scope is still
    /// one. No entry here can name registrations that are gone: each is being resolved
    /// from further up this call stack, which is what holds them alive.
    static CONSTRUCTING: RefCell<Vec<(usize, TypeId, &'static str)>> =
        const { RefCell::new(Vec::new()) };
}

/// Marks a service as under construction on this thread for as long as it lives, and
/// takes it off again even when construction unwinds
struct Constructing;

impl Constructing {
    /// # Panics
    /// if `T` is already under construction on this thread out of the same registrations:
    /// a dependency cycle
    #[inline]
    fn enter<T: 'static>(graph: usize) -> Self {
        let id = TypeId::of::<T>();
        let name = std::any::type_name::<T>();

        // Build the report inside the borrow and panic outside it, so nothing unwinding
        // from here finds the stack still borrowed
        let cycle = CONSTRUCTING.with_borrow_mut(|stack| {
            let entered = stack
                .iter()
                .position(|(from, entered, _)| *from == graph && *entered == id);

            if let Some(start) = entered {
                let mut path = stack[start..]
                    .iter()
                    .map(|(_, _, name)| *name)
                    .collect::<Vec<_>>();

                path.push(name);

                return Some(path.join(" -> "));
            }
            stack.push((graph, id, name));
            None
        });

        if let Some(path) = cycle {
            panic!(
                "volga-di: dependency cycle: {path}. Declare what each service resolves in \
                 `Inject::dependencies` to have `ContainerBuilder::validate` report it before \
                 anything is resolved"
            );
        }
        Self
    }
}

impl Drop for Constructing {
    #[inline]
    fn drop(&mut self) {
        let _ = CONSTRUCTING.try_with(|stack| stack.borrow_mut().pop());
    }
}

/// What a registration declares it resolves, kept for [`ContainerBuilder::validate`]
#[derive(Debug)]
struct Declared {
    name: &'static str,
    dependencies: Vec<Dependency>,
}

/// Where a service stands in the walk over the declared graph
#[derive(Clone, Copy, PartialEq, Eq)]
enum Visit {
    InProgress,
    Done,
}

/// Inner HashMap of dependencies
type ServiceMap = HashMap<TypeId, ServiceEntry, BuildHasherDefault<TypeIdHasher>>;

/// A hasher for types in DI container
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
    services: ServiceMap,

    /// What each registration declares it resolves. Kept by the builder alone: only
    /// [`ContainerBuilder::validate`] reads it, and a built container has no use for it
    declared: HashMap<TypeId, Declared, BuildHasherDefault<TypeIdHasher>>,
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
        Self {
            services: ServiceMap::default(),
            declared: HashMap::default(),
        }
    }

    /// Checks the dependency graph the registrations declare, before anything is resolved.
    ///
    /// Reports every service that takes part in a cycle and every declared dependency that
    /// nothing registered. What a registration declares is what its factory's arguments
    /// or its [`Inject::dependencies`] say it resolves; a type that declares nothing is not
    /// checked here, and a cycle through it is stopped when it is resolved instead.
    ///
    /// # Errors
    /// Returns a [`ValidationError`] listing every problem found.
    ///
    /// # Example
    /// ```
    /// use volga_di::ContainerBuilder;
    ///
    /// let mut builder = ContainerBuilder::new();
    /// builder.register_singleton(42u32);
    ///
    /// assert!(builder.validate().is_ok());
    /// let container = builder.build();
    /// ```
    pub fn validate(&self) -> Result<(), ValidationError> {
        // Walk the services in a stable order: the map's own order follows type ids, which
        // change from one build to the next
        let mut roots = self
            .declared
            .iter()
            .map(|(id, declared)| (*id, declared.name))
            .collect::<Vec<_>>();

        roots.sort_unstable_by_key(|(_, name)| *name);

        let mut issues = Vec::new();
        let mut visits = HashMap::<TypeId, Visit, BuildHasherDefault<TypeIdHasher>>::default();
        let mut path = Vec::new();
        for (id, _) in &roots {
            self.visit(*id, &mut visits, &mut path, &mut issues);
        }

        for (id, name) in &roots {
            let Some(declared) = self.declared.get(id) else {
                continue;
            };

            for dependency in &declared.dependencies {
                let issue = Issue::Missing {
                    service: name,
                    dependency: dependency.name,
                };

                if !self.declared.contains_key(&dependency.id) && !issues.contains(&issue) {
                    issues.push(issue);
                }
            }
        }

        if issues.is_empty() {
            Ok(())
        } else {
            Err(ValidationError::new(issues))
        }
    }

    /// Depth-first walk from `id`, reporting a cycle wherever it reaches a service still
    /// on the current path
    fn visit(
        &self,
        id: TypeId,
        visits: &mut HashMap<TypeId, Visit, BuildHasherDefault<TypeIdHasher>>,
        path: &mut Vec<(TypeId, &'static str)>,
        issues: &mut Vec<Issue>,
    ) {
        // A dependency nobody registered has nothing to walk; it is reported on its own
        let Some(declared) = self.declared.get(&id) else {
            return;
        };

        match visits.get(&id) {
            Some(Visit::Done) => return,
            Some(Visit::InProgress) => {
                if let Some(start) = path.iter().position(|(on_path, _)| *on_path == id) {
                    let mut cycle = path[start..]
                        .iter()
                        .map(|(_, name)| *name)
                        .collect::<Vec<_>>();

                    cycle.push(declared.name);
                    issues.push(Issue::Cycle(cycle));
                }
                return;
            }
            None => {}
        }

        visits.insert(id, Visit::InProgress);
        path.push((id, declared.name));
        for dependency in &declared.dependencies {
            self.visit(dependency.id, visits, path, issues);
        }
        path.pop();
        visits.insert(id, Visit::Done);
    }

    /// Records what the registration of `T` declares, replacing what an earlier
    /// registration of the same type declared
    #[inline]
    fn declare<T: 'static>(&mut self, dependencies: Vec<Dependency>) {
        self.declared.insert(
            TypeId::of::<T>(),
            Declared {
                name: std::any::type_name::<T>(),
                dependencies,
            },
        );
    }

    /// Build a DI container
    #[inline]
    pub fn build(mut self) -> Container {
        // Number the scoped registrations here rather than as they are made: registering
        // a type again replaces its entry, which would leave a hole in the numbering
        let mut len = 0;
        for entry in self.services.values_mut() {
            if let ServiceEntry::Scoped(slot, _) = entry {
                *slot = len;
                len += 1;
            }
        }

        Container {
            services: Arc::new(self.services),
            scoped: (len > 0).then(|| scoped_cells(len)),
        }
    }

    /// Register a singleton service
    pub fn register_singleton<T: Send + Sync + 'static>(&mut self, instance: T) {
        self.services
            .insert(TypeId::of::<T>(), ServiceEntry::singleton(instance));

        self.declare::<T>(Vec::new());
    }

    /// Register a scoped service
    pub fn register_scoped_factory<T, F, Args>(&mut self, factory: F)
    where
        T: Send + Sync + 'static,
        F: GenericFactory<Args, Output = T>,
        Args: Inject,
    {
        self.services.insert(
            TypeId::of::<T>(),
            ServiceEntry::scoped(make_resolver_fn(factory)),
        );

        self.declare::<T>(Dependencies::of::<Args>());
    }

    /// Register a transient service that required to be resolved as [`Default`]
    pub fn register_scoped_default<T>(&mut self)
    where
        T: Default + Send + Sync + 'static,
    {
        self.register_scoped_factory(T::default);
    }

    /// Register a transient service that required to be resolved as [`Inject`]
    pub fn register_scoped<T: Inject + 'static>(&mut self) {
        self.services.insert(
            TypeId::of::<T>(),
            ServiceEntry::scoped(make_inject_resolver_fn::<T>()),
        );

        self.declare::<T>(Dependencies::of::<T>());
    }

    /// Register a transient service
    pub fn register_transient_factory<T, F, Args>(&mut self, factory: F)
    where
        T: Send + Sync + 'static,
        F: GenericFactory<Args, Output = T>,
        Args: Inject,
    {
        self.services.insert(
            TypeId::of::<T>(),
            ServiceEntry::transient(make_resolver_fn(factory)),
        );

        self.declare::<T>(Dependencies::of::<Args>());
    }

    /// Register a transient service that required to be resolved as [`Default`]
    pub fn register_transient_default<T>(&mut self)
    where
        T: Default + Send + Sync + 'static,
    {
        self.register_transient_factory(T::default);
    }

    /// Register a transient service that required to be resolved as [`Inject`]
    pub fn register_transient<T: Inject + 'static>(&mut self) {
        self.services.insert(
            TypeId::of::<T>(),
            ServiceEntry::transient(make_inject_resolver_fn::<T>()),
        );

        self.declare::<T>(Dependencies::of::<T>());
    }
}

/// Represents a DI container, that is able to resolve generic dependencies
#[derive(Debug, Clone)]
pub struct Container {
    /// Read-only HashMap of dependencies, shared by the root container and every scope
    /// created from it: registrations do not change once the container is built
    services: Arc<ServiceMap>,

    /// This scope's instances of the scoped services - one cell per scoped registration,
    /// at the index its entry carries. `None` where nothing is registered scoped.
    ///
    /// Shared rather than owned: a clone of a container is the same scope, not a snapshot
    /// of it, so an instance resolved through either has to be the one both see.
    scoped: Option<Arc<[ScopedCell]>>,
}

impl Container {
    /// Creates a new child dependency-injection scope that inherits all service
    /// registrations from its parent:
    ///
    /// - **Singleton** services are shared: the child scope reuses the parent's
    ///   singleton instances.
    /// - **Scoped** services are isolated: they are not instantiated upfront and
    ///   will be lazily created the first time they are resolved within this scope.
    /// - **Transient** services preserve their lifetime semantics: each resolution
    ///   returns a newly constructed instance.
    ///
    /// This method is typically used to create request-level or operation-level
    /// scopes when resolving services that should not live for the entire lifetime
    /// of the root container.
    ///
    /// A server calls this once per request, before it knows whether the request will
    /// resolve anything at all, so what it costs is paid by every request. A scope shares
    /// the registrations with its parent and gets nothing of its own but an empty cell
    /// per scoped service, so the cost follows the number of scoped registrations rather
    /// than the number of registrations - and is a single atomic increment where nothing
    /// is registered scoped.
    #[inline]
    pub fn create_scope(&self) -> Self {
        Self {
            services: Arc::clone(&self.services),
            scoped: self.scoped.as_ref().map(|cells| scoped_cells(cells.len())),
        }
    }

    /// Resolves a service and returns a cloned instance.
    /// `T` must implement [`Clone`] otherwise use [`Container::resolve_shared`] method
    /// that returns a shared pointer.
    ///
    /// A shared instance - a singleton, or a scoped service already built in this scope - is
    /// cloned where it lies, without touching the [`Arc`] that holds it: every thread
    /// resolving the service writes that count, and bumping it only to drop it again is
    /// contention for nothing. A transient is built for this call alone, so it is handed
    /// over as built rather than cloned.
    ///
    /// # Panics
    /// if resolving `T` leads back to `T` - see [`Container::resolve_shared`].
    #[inline]
    pub fn resolve<T: Send + Sync + Clone + 'static>(&self) -> Result<T, Error> {
        match self.get_service_entry::<T>()? {
            ServiceEntry::Transient(r) => self.construct::<T>(r).map(Arc::unwrap_or_clone),
            ServiceEntry::Scoped(slot, r) => self
                .scoped_instance::<T>(*slot, r)
                .and_then(Self::downcast_ref::<T>)
                .cloned(),
            ServiceEntry::Singleton(instance) => Self::downcast_ref::<T>(instance).cloned(),
        }
    }

    /// Resolves a service and returns a shared pointer
    ///
    /// # Panics
    /// if constructing `T` leads back to constructing `T` on the same thread: a dependency
    /// cycle that [`ContainerBuilder::validate`] could not see, because a type on it does not
    /// declare what it resolves. The panic names every service on the loop. Resolving a
    /// singleton, or a scoped service already built in this scope, never gets this far.
    #[inline]
    pub fn resolve_shared<T: Send + Sync + 'static>(&self) -> Result<Arc<T>, Error> {
        match self.get_service_entry::<T>()? {
            ServiceEntry::Transient(r) => self.construct::<T>(r),
            ServiceEntry::Scoped(slot, r) => self
                .scoped_instance::<T>(*slot, r)
                .and_then(Self::downcast_shared::<T>),
            ServiceEntry::Singleton(instance) => Self::downcast_shared::<T>(instance),
        }
    }

    /// Identifies the registrations this container resolves out of: the same for every scope
    /// created from it, and not the same as those of a container built anywhere else
    #[inline]
    fn graph(&self) -> usize {
        Arc::as_ptr(&self.services) as usize
    }

    /// Fetches the service entry or return an error if not registered.
    #[inline]
    fn get_service_entry<T: Send + Sync + 'static>(&self) -> Result<&ServiceEntry, Error> {
        let type_id = TypeId::of::<T>();
        self.services
            .get(&type_id)
            .ok_or_else(|| Error::NotRegistered(std::any::type_name::<T>()))
    }

    /// Builds a transient service for the caller alone
    #[inline]
    fn construct<T: Send + Sync + 'static>(
        &self,
        resolver_fn: &ResolverFn,
    ) -> Result<Arc<T>, Error> {
        let _constructing = Constructing::enter::<T>(self.graph());
        // Just built and held by nothing else, so it is downcast as it is, not through
        // another handle on it
        resolver_fn(self)?
            .downcast::<T>()
            .map_err(|_| Error::ResolveFailed(std::any::type_name::<T>()))
    }

    /// This scope's instance of a scoped service, built the first time it is asked for
    #[inline]
    fn scoped_instance<T: 'static>(
        &self,
        slot: usize,
        resolver_fn: &ResolverFn,
    ) -> Result<&ArcService, Error> {
        // Every scope is built from the map this entry came from, so it holds a cell at
        // every index that map hands out. A miss would be a bug in `build` rather than a
        // state a caller can reach - reported as a failed resolution, not a panic
        let cell = self
            .scoped
            .as_deref()
            .and_then(|cells| cells.get(slot))
            .ok_or_else(|| Error::ResolveFailed(std::any::type_name::<T>()))?;

        let result = match cell.get() {
            Some(result) => result,
            None => {
                // Enter before `get_or_init`, not inside it: a cycle comes back to this very
                // cell, and `OnceLock` deadlocks on reentrant initialization before the
                // closure would get a chance to notice
                let _constructing = Constructing::enter::<T>(self.graph());
                cell.get_or_init(|| resolver_fn(self))
            }
        };

        result.as_ref().map_err(|err| *err)
    }

    /// Borrows `T` out of a shared instance
    #[inline]
    fn downcast_ref<T: 'static>(instance: &ArcService) -> Result<&T, Error> {
        // `**` to ask the value, not the `Arc` handle - which is `Any` as well
        (**instance)
            .downcast_ref::<T>()
            .ok_or_else(|| Error::ResolveFailed(std::any::type_name::<T>()))
    }

    /// Unwraps `T` from [`ArcService`] as another handle on the shared instance
    #[inline]
    fn downcast_shared<T: Send + Sync + 'static>(instance: &ArcService) -> Result<Arc<T>, Error> {
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
        extensions.get::<Container>().ok_or(Error::ContainerMissing)
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
    use super::{Container, ContainerBuilder, Error, Inject};
    use crate::{error::Issue, inject::Dependencies};
    use http::Request;
    use std::any::type_name;
    use std::collections::HashMap;
    use std::marker::PhantomData;
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
    use std::sync::{Arc, Mutex, mpsc};
    use std::time::Duration;

    trait Cache: Send + Sync {
        fn get(&self, key: &str) -> Option<String>;
        fn set(&self, key: &str, value: &str);
    }

    #[derive(Clone, Default)]
    struct InMemoryCache {
        inner: Arc<Mutex<HashMap<String, String>>>,
    }

    impl Cache for InMemoryCache {
        fn get(&self, key: &str) -> Option<String> {
            self.inner.lock().unwrap().get(key).cloned()
        }

        fn set(&self, key: &str, value: &str) {
            self.inner
                .lock()
                .unwrap()
                .insert(key.to_string(), value.to_string());
        }
    }

    #[derive(Clone, Default)]
    struct InMemoryCache2(InMemoryCache);

    #[derive(Default)]
    struct First(u8);

    #[derive(Default)]
    struct Second;

    #[derive(Default)]
    struct Third;

    #[derive(Clone)]
    struct CacheWrapper {
        inner: InMemoryCache,
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
        let container = ContainerBuilder::new().build().create_scope();

        let cache = container.resolve::<CacheWrapper>();

        assert!(cache.is_err());
    }

    #[test]
    fn it_keeps_lifetimes_in_a_scope_of_a_container_with_nothing_scoped() {
        let mut container = ContainerBuilder::new();
        container.register_singleton(InMemoryCache::default());
        container.register_transient_default::<InMemoryCache2>();

        let container = container.build();
        let scope = container.create_scope();

        // The singleton is the parent's instance, as it is in any scope
        scope
            .resolve::<InMemoryCache>()
            .unwrap()
            .set("key", "value");
        assert_eq!(
            container
                .resolve::<InMemoryCache>()
                .unwrap()
                .get("key")
                .unwrap(),
            "value"
        );

        // The transient is still built fresh on every resolution, even though this
        // scope shares the parent's registrations rather than copying them
        scope
            .resolve::<InMemoryCache2>()
            .unwrap()
            .0
            .set("key", "value");
        assert!(
            scope
                .resolve::<InMemoryCache2>()
                .unwrap()
                .0
                .get("key")
                .is_none()
        );
    }

    #[test]
    fn it_isolates_a_scoped_service_registered_beside_ones_that_are_not() {
        let mut container = ContainerBuilder::new();
        container.register_singleton(InMemoryCache::default());
        container.register_scoped_default::<InMemoryCache2>();

        let container = container.build();

        let first = container.create_scope();
        first.resolve::<InMemoryCache2>().unwrap().0.set("key", "1");
        assert_eq!(
            first
                .resolve::<InMemoryCache2>()
                .unwrap()
                .0
                .get("key")
                .unwrap(),
            "1"
        );

        // One scoped registration is enough to give every scope cells of its own
        let second = container.create_scope();
        assert!(
            second
                .resolve::<InMemoryCache2>()
                .unwrap()
                .0
                .get("key")
                .is_none()
        );
    }

    #[test]
    fn it_shares_scoped_instances_between_clones_of_one_scope() {
        let mut container = ContainerBuilder::new();
        container.register_scoped_default::<InMemoryCache>();

        let container = container.build();
        let scope = container.create_scope();

        // A clone of a scope is that scope - the way a request's container reaches a
        // handler and an `Inject` impl - so whichever resolves first builds the one
        // instance both see
        let through_clone = scope.clone().resolve_shared::<InMemoryCache>().unwrap();
        let through_scope = scope.resolve_shared::<InMemoryCache>().unwrap();

        assert!(Arc::ptr_eq(&through_clone, &through_scope));
    }

    #[test]
    fn it_gives_each_scoped_service_a_cell_of_its_own() {
        let mut container = ContainerBuilder::new();
        container.register_scoped_default::<First>();
        container.register_singleton(InMemoryCache::default());
        container.register_scoped_default::<Second>();
        container.register_transient_default::<InMemoryCache2>();
        container.register_scoped_default::<Third>();

        let container = container.build();
        let scope = container.create_scope();

        // Two services sharing a cell would hand one of them the other's instance, and the
        // downcast would fail
        let first = scope.resolve_shared::<First>().unwrap();
        let second = scope.resolve_shared::<Second>().unwrap();
        let third = scope.resolve_shared::<Third>().unwrap();

        assert!(Arc::ptr_eq(
            &first,
            &scope.resolve_shared::<First>().unwrap()
        ));
        assert!(Arc::ptr_eq(
            &second,
            &scope.resolve_shared::<Second>().unwrap()
        ));
        assert!(Arc::ptr_eq(
            &third,
            &scope.resolve_shared::<Third>().unwrap()
        ));
    }

    #[test]
    fn it_numbers_scoped_services_after_they_are_registered_again() {
        let mut container = ContainerBuilder::new();
        container.register_scoped_default::<First>();
        container.register_scoped_default::<Second>();
        // `First` stops being scoped, `Second` is registered scoped a second time
        container.register_singleton(First(7));
        container.register_scoped_default::<Second>();

        let container = container.build();
        let one = container.create_scope();
        let other = container.create_scope();

        assert!(Arc::ptr_eq(
            &one.resolve_shared::<First>().unwrap(),
            &other.resolve_shared::<First>().unwrap()
        ));
        assert_eq!(one.resolve_shared::<First>().unwrap().0, 7);
        assert!(!Arc::ptr_eq(
            &one.resolve_shared::<Second>().unwrap(),
            &other.resolve_shared::<Second>().unwrap()
        ));
    }

    #[test]
    fn it_isolates_a_scope_created_from_a_scope() {
        let mut container = ContainerBuilder::new();
        container.register_scoped_default::<First>();

        let container = container.build();
        let outer = container.create_scope();
        let inner = outer.create_scope();

        assert!(!Arc::ptr_eq(
            &outer.resolve_shared::<First>().unwrap(),
            &inner.resolve_shared::<First>().unwrap()
        ));
        assert!(!Arc::ptr_eq(
            &container.resolve_shared::<First>().unwrap(),
            &outer.resolve_shared::<First>().unwrap()
        ));
    }

    #[test]
    fn it_resolves_a_transient_against_the_scope_it_is_resolved_in() {
        let mut container = ContainerBuilder::new();
        container.register_scoped_default::<InMemoryCache>();
        container.register_transient::<CacheWrapper>();

        let container = container.build();

        // Two transients built in one scope reach the one scoped cache underneath
        let scope = container.create_scope();
        scope
            .resolve::<CacheWrapper>()
            .unwrap()
            .inner
            .set("key", "value");
        assert_eq!(
            scope
                .resolve::<CacheWrapper>()
                .unwrap()
                .inner
                .get("key")
                .unwrap(),
            "value"
        );

        // and a transient built in another scope reaches that scope's cache
        let other = container.create_scope();
        assert!(
            other
                .resolve::<CacheWrapper>()
                .unwrap()
                .inner
                .get("key")
                .is_none()
        );
    }
    /// Resolves `T` and declares it, the way `Dc<T>` does in volga. Keeps nothing: these
    /// tests care that `T` is resolved, not about the instance
    struct Shared<T>(PhantomData<fn() -> T>);

    impl<T: Send + Sync + 'static> Inject for Shared<T> {
        fn inject(container: &Container) -> Result<Self, Error> {
            container.resolve_shared::<T>().map(|_| Shared(PhantomData))
        }

        fn dependencies(deps: &mut Dependencies) {
            deps.add::<T>();
        }
    }

    struct Left;
    struct Right;
    struct Unregistered;

    /// Resolves itself, and says so
    struct DeclaredLoop;

    impl Inject for DeclaredLoop {
        fn inject(container: &Container) -> Result<Self, Error> {
            container.resolve_shared::<DeclaredLoop>()?;
            Ok(Self)
        }

        fn dependencies(deps: &mut Dependencies) {
            deps.add::<DeclaredLoop>();
        }
    }

    /// Resolves something nobody registered, without saying so
    struct Opaque;

    impl Inject for Opaque {
        fn inject(container: &Container) -> Result<Self, Error> {
            container.resolve_shared::<Unregistered>()?;
            Ok(Self)
        }
    }

    /// Resolves itself without saying so
    struct SelfLoop;

    impl Inject for SelfLoop {
        fn inject(container: &Container) -> Result<Self, Error> {
            container.resolve_shared::<SelfLoop>()?;
            Ok(Self)
        }
    }

    /// Resolves itself out of a scope of the very container it is being built in
    struct ThroughScope;

    impl Inject for ThroughScope {
        fn inject(container: &Container) -> Result<Self, Error> {
            container.create_scope().resolve_shared::<ThroughScope>()?;
            Ok(Self)
        }
    }

    /// A service type that more than one container registers
    #[derive(Clone)]
    struct Tenant(&'static str);

    struct Ping;
    struct Pong;

    impl Inject for Ping {
        fn inject(container: &Container) -> Result<Self, Error> {
            container.resolve_shared::<Pong>()?;
            Ok(Self)
        }
    }

    impl Inject for Pong {
        fn inject(container: &Container) -> Result<Self, Error> {
            container.resolve_shared::<Ping>()?;
            Ok(Self)
        }
    }

    static FLAKY_RECURSES: AtomicBool = AtomicBool::new(true);

    /// Resolves itself the first time it is built, and not after
    struct Flaky;

    impl Inject for Flaky {
        fn inject(container: &Container) -> Result<Self, Error> {
            if FLAKY_RECURSES.swap(false, Ordering::SeqCst) {
                container.resolve_shared::<Flaky>()?;
            }
            Ok(Self)
        }
    }

    struct Slow;

    /// Runs `f` on a thread of its own and reports how it ended: the panic message, or
    /// `None` when it returned. A resolution that deadlocks fails the test after a few
    /// seconds rather than hanging the suite.
    fn outcome(f: impl FnOnce() + Send + 'static) -> Option<String> {
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            let panic = catch_unwind(AssertUnwindSafe(f)).err().map(|payload| {
                payload
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| payload.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_default()
            });
            let _ = tx.send(panic);
        });
        rx.recv_timeout(Duration::from_secs(5))
            .expect("the resolution never finished - a deadlock")
    }

    #[test]
    fn it_validates_a_graph_that_resolves() {
        let mut builder = ContainerBuilder::new();
        builder.register_singleton(InMemoryCache::default());
        builder.register_scoped_factory(|_: Shared<InMemoryCache>| Ok(Left));
        builder.register_transient_factory(|_: Shared<Left>, _: Shared<InMemoryCache>| Ok(Right));

        assert_eq!(builder.validate(), Ok(()));
    }

    #[test]
    fn it_reports_a_cycle_between_factories() {
        let mut builder = ContainerBuilder::new();
        builder.register_scoped_factory(|_: Shared<Right>| Ok(Left));
        builder.register_transient_factory(|_: Shared<Left>| Ok(Right));

        let err = builder.validate().unwrap_err();
        assert_eq!(
            err.issues(),
            [Issue::Cycle(vec![
                type_name::<Left>(),
                type_name::<Right>(),
                type_name::<Left>()
            ])]
        );
    }

    #[test]
    fn it_reports_a_service_that_depends_on_itself() {
        let mut builder = ContainerBuilder::new();
        builder.register_scoped::<DeclaredLoop>();

        let err = builder.validate().unwrap_err();
        assert_eq!(
            err.issues(),
            [Issue::Cycle(vec![
                type_name::<DeclaredLoop>(),
                type_name::<DeclaredLoop>()
            ])]
        );
    }

    #[test]
    fn it_reports_a_dependency_nobody_registered() {
        let mut builder = ContainerBuilder::new();
        builder.register_singleton(InMemoryCache::default());
        // One argument fine, one missing, the missing one twice
        builder.register_scoped_factory(
            |_: Shared<InMemoryCache>, _: Shared<Unregistered>, _: Shared<Unregistered>| Ok(Left),
        );

        let err = builder.validate().unwrap_err();
        assert_eq!(
            err.issues(),
            [Issue::Missing {
                service: type_name::<Left>(),
                dependency: type_name::<Unregistered>(),
            }]
        );
    }

    #[test]
    fn it_reports_every_problem_at_once() {
        let mut builder = ContainerBuilder::new();
        builder.register_scoped_factory(|_: Shared<Right>| Ok(Left));
        builder.register_scoped_factory(|_: Shared<Left>, _: Shared<Unregistered>| Ok(Right));

        let err = builder.validate().unwrap_err();
        assert_eq!(
            err.issues(),
            [
                Issue::Cycle(vec![
                    type_name::<Left>(),
                    type_name::<Right>(),
                    type_name::<Left>()
                ]),
                Issue::Missing {
                    service: type_name::<Right>(),
                    dependency: type_name::<Unregistered>(),
                },
            ]
        );
        assert_eq!(
            err.to_string(),
            format!(
                "dependency injection: dependency cycle: {l} -> {r} -> {l}; `{r}` depends on `{u}`, which is not registered",
                l = type_name::<Left>(),
                r = type_name::<Right>(),
                u = type_name::<Unregistered>()
            )
        );
    }

    #[test]
    fn it_forgets_what_a_replaced_registration_declared() {
        let mut builder = ContainerBuilder::new();
        builder.register_scoped_factory(|_: Shared<Unregistered>| Ok(Left));
        builder.register_singleton(Left);

        assert_eq!(builder.validate(), Ok(()));
    }

    #[test]
    fn it_leaves_a_type_that_declares_nothing_out_of_the_check() {
        let mut builder = ContainerBuilder::new();
        builder.register_scoped::<Opaque>();

        // What `Opaque` resolves cannot be seen from here, so there is nothing to report
        assert_eq!(builder.validate(), Ok(()));
    }

    #[test]
    fn it_panics_on_a_scoped_service_that_resolves_itself() {
        let message = outcome(|| {
            let mut builder = ContainerBuilder::new();
            builder.register_scoped::<SelfLoop>();
            let _ = builder.build().create_scope().resolve_shared::<SelfLoop>();
        })
        .expect("the cycle was not reported");

        let loop_ = type_name::<SelfLoop>();
        assert!(
            message.contains(&format!("dependency cycle: {loop_} -> {loop_}")),
            "{message}"
        );
    }

    #[test]
    fn it_panics_on_a_transient_that_resolves_itself() {
        // Without the check this overflows the stack, which aborts the whole process
        let message = outcome(|| {
            let mut builder = ContainerBuilder::new();
            builder.register_transient::<SelfLoop>();
            let _ = builder.build().resolve_shared::<SelfLoop>();
        })
        .expect("the cycle was not reported");

        assert!(message.contains("dependency cycle"), "{message}");
    }

    #[test]
    fn it_panics_on_a_cycle_through_two_scoped_services() {
        let message = outcome(|| {
            let mut builder = ContainerBuilder::new();
            builder.register_scoped::<Ping>();
            builder.register_scoped::<Pong>();
            let _ = builder.build().create_scope().resolve_shared::<Ping>();
        })
        .expect("the cycle was not reported");

        let (ping, pong) = (type_name::<Ping>(), type_name::<Pong>());
        assert!(
            message.contains(&format!("{ping} -> {pong} -> {ping}")),
            "{message}"
        );
    }

    #[test]
    fn it_panics_on_a_cycle_that_mixes_lifetimes() {
        let message = outcome(|| {
            let mut builder = ContainerBuilder::new();
            builder.register_scoped::<Ping>();
            builder.register_transient::<Pong>();
            let _ = builder.build().create_scope().resolve_shared::<Pong>();
        })
        .expect("the cycle was not reported");

        let (ping, pong) = (type_name::<Ping>(), type_name::<Pong>());
        assert!(
            message.contains(&format!("{pong} -> {ping} -> {pong}")),
            "{message}"
        );
    }

    #[test]
    fn it_reports_a_cycle_that_goes_through_a_scope_of_the_same_container() {
        // A scope carries the registrations it was created from, so this is one graph
        let message = outcome(|| {
            let mut builder = ContainerBuilder::new();
            builder.register_transient::<ThroughScope>();
            let _ = builder.build().resolve_shared::<ThroughScope>();
        })
        .expect("the cycle was not reported");

        let through_scope = type_name::<ThroughScope>();
        assert!(
            message.contains(&format!("{through_scope} -> {through_scope}")),
            "{message}"
        );
    }

    #[test]
    fn it_does_not_report_a_cycle_across_independent_containers() {
        let panicked = outcome(|| {
            let mut inner = ContainerBuilder::new();
            inner.register_transient_factory(|| Tenant("inner"));
            let inner = inner.build();

            // The outer container builds its own `Tenant` out of the one the inner container
            // resolves - a second graph that happens to carry the same service type
            let mut outer = ContainerBuilder::new();
            outer.register_transient_factory(move || {
                let Tenant(name) = inner.resolve::<Tenant>().expect("the inner container");
                Tenant(name)
            });

            let tenant = outer
                .build()
                .resolve::<Tenant>()
                .expect("the outer container");

            assert_eq!(tenant.0, "inner");
        });

        assert_eq!(panicked, None, "two containers are not one graph");
    }

    #[test]
    fn it_leaves_the_thread_usable_after_a_cycle() {
        let panicked = outcome(|| {
            let mut builder = ContainerBuilder::new();
            builder.register_transient::<Flaky>();
            let container = builder.build();

            // The first build of `Flaky` loops back on itself and panics
            let first = catch_unwind(AssertUnwindSafe(|| container.resolve_shared::<Flaky>()));
            assert!(first.is_err());

            // The unwinding took `Flaky` off this thread's stack again, so the next build -
            // which does not loop - is not mistaken for one
            assert!(container.resolve_shared::<Flaky>().is_ok());
        });

        assert_eq!(panicked, None);
    }

    #[test]
    fn it_lets_threads_wait_on_a_scoped_service_another_is_building() {
        let mut builder = ContainerBuilder::new();
        builder.register_scoped_factory(|| {
            std::thread::sleep(Duration::from_millis(50));
            Slow
        });
        let scope = builder.build().create_scope();

        // Waiting for another thread's construction is not a cycle
        let instances = std::thread::scope(|s| {
            let handles = (0..4)
                .map(|_| s.spawn(|| scope.resolve_shared::<Slow>().unwrap()))
                .collect::<Vec<_>>();
            handles
                .into_iter()
                .map(|h| h.join().unwrap())
                .collect::<Vec<_>>()
        });

        assert!(instances.windows(2).all(|w| Arc::ptr_eq(&w[0], &w[1])));
    }
    static TRANSIENT_CLONES: AtomicUsize = AtomicUsize::new(0);

    #[derive(Default)]
    struct CountedTransient;

    impl Clone for CountedTransient {
        fn clone(&self) -> Self {
            TRANSIENT_CLONES.fetch_add(1, Ordering::SeqCst);
            Self
        }
    }

    static SINGLETON_CLONES: AtomicUsize = AtomicUsize::new(0);

    struct CountedSingleton(u8);

    impl Clone for CountedSingleton {
        fn clone(&self) -> Self {
            SINGLETON_CLONES.fetch_add(1, Ordering::SeqCst);
            Self(self.0)
        }
    }

    #[test]
    fn it_hands_over_a_transient_without_cloning_it() {
        let mut builder = ContainerBuilder::new();
        builder.register_transient_default::<CountedTransient>();
        let container = builder.build();

        // Each is built for its caller alone, so there is nothing to clone it from
        let _ = container.resolve::<CountedTransient>().unwrap();
        let _ = container.resolve_shared::<CountedTransient>().unwrap();

        assert_eq!(TRANSIENT_CLONES.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn it_clones_a_singleton_for_resolve_and_shares_it_for_resolve_shared() {
        let mut builder = ContainerBuilder::new();
        builder.register_singleton(CountedSingleton(7));
        let container = builder.build();

        assert_eq!(container.resolve::<CountedSingleton>().unwrap().0, 7);
        assert_eq!(SINGLETON_CLONES.load(Ordering::SeqCst), 1);

        let shared = container.resolve_shared::<CountedSingleton>().unwrap();
        let from_scope = container
            .create_scope()
            .resolve_shared::<CountedSingleton>()
            .unwrap();
        assert!(Arc::ptr_eq(&shared, &from_scope));
        assert_eq!(SINGLETON_CLONES.load(Ordering::SeqCst), 1);
    }
}
