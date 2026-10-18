//! Tools for Dependency Injection
//!
//! A service's lifetime decides who shares an instance of it:
//!
//! - a **singleton** ([`App::add_singleton`]) - one instance for the whole app, shared by
//!   every request at once;
//! - a **scoped** service ([`App::add_scoped`] and the other `add_scoped_*` methods) - one
//!   instance per request, shared by every handler argument and middleware that resolves it
//!   during that request;
//! - a **transient** service ([`App::add_transient`] and the other `add_transient_*`
//!   methods) - a new instance every time it is resolved.
//!
//! [`Dc<T>`] gives a handler `&T`. A singleton or a scoped service whose state changes while
//! the app runs keeps that state behind interior mutability - a [`Mutex`](std::sync::Mutex),
//! an [`RwLock`](std::sync::RwLock), an atomic - and changes it through `&self`. See [`Dc`]
//! for an example.
//!
//! [`App::run`] checks the registrations before it starts. It refuses to start on a
//! dependency cycle, on a service depending on one nobody registered, and on a handler or a
//! middleware taking [`Dc<T>`] of a `T` nobody registered - which would otherwise answer
//! `500` on every request that reached it. `Option<Dc<T>>` takes a service that may not be
//! registered. A service resolved while a request runs - through `HttpContext::resolve` or
//! the [`Container`] itself - cannot be seen at startup.

use super::{App, error::Error};
use std::{borrow::Cow, sync::Arc};
pub use {
    self::dc::Dc,
    volga_di::{Container, ContainerBuilder, Dependencies, GenericFactory, Inject},
};

pub mod dc;

/// Dependency injection errors
pub mod error {
    pub use volga_di::error::{Error, Issue, ValidationError};
}

/// Specifies extractors to read data from a borrowed DI container
pub trait FromContainer: Sized {
    /// Extracts data from DI container
    fn from_container(container: &Container) -> Result<Self, Error>;

    /// Declares the services this extractor resolves from the container.
    ///
    /// [`App::run`] checks that each of them is registered before it starts, so that
    /// whatever takes this extractor does not answer `500` on every request instead. The
    /// default declares nothing. An extractor of your own declares here what
    /// [`from_container`](Self::from_container) resolves.
    #[inline]
    fn dependencies(_deps: &mut Dependencies) {}
}

impl FromContainer for () {
    #[inline]
    fn from_container(_: &Container) -> Result<Self, Error> {
        Ok(())
    }
}

impl crate::error::IntoError for error::Error {
    #[inline]
    fn into_error(self) -> Error {
        Error::server_error(self.to_string())
    }
}

/// What a part of the app resolves from the container, kept until the app starts together
/// with a weak handle on that part.
///
/// Some parts can still be replaced or left out after they are registered: a `map_err` or a
/// fallback mapped again, a route group's middleware attached to nothing. A part the app no
/// longer holds when it starts can never run, so what it resolves is not checked.
pub(crate) struct HeldRequirement {
    held: Box<dyn Fn() -> bool + Send + Sync>,
    consumer: Cow<'static, str>,
    declare: fn(&mut Dependencies),
}

impl std::fmt::Debug for HeldRequirement {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HeldRequirement")
            .field("consumer", &self.consumer)
            .finish_non_exhaustive()
    }
}

impl App {
    /// Declares what `part` resolves from the container, for [`App::run`] to check if the
    /// app still holds `part` when it starts
    pub(crate) fn require_while_held<T>(
        &mut self,
        part: &Arc<T>,
        consumer: impl Into<Cow<'static, str>>,
        declare: fn(&mut Dependencies),
    ) where
        T: ?Sized + Send + Sync + 'static,
    {
        let part = Arc::downgrade(part);
        self.held_requirements.push(HeldRequirement {
            held: Box::new(move || part.strong_count() > 0),
            consumer: consumer.into(),
            declare,
        });
    }

    /// Declares to the container what the parts the app still holds resolve
    pub(crate) fn declare_held_requirements(mut self) -> Self {
        for requirement in std::mem::take(&mut self.held_requirements) {
            if (requirement.held)() {
                self.container
                    .require(requirement.consumer, requirement.declare);
            }
        }
        self
    }
}

/// DI specific impl for [`App`]
impl App {
    /// Registers singleton service
    ///
    /// > **Note:** every request shares this one instance, and [`Dc<T>`] gives it as `&T`.
    /// > State that changes after registration goes behind interior mutability inside `T`:
    /// > a [`Mutex`](std::sync::Mutex), an [`RwLock`](std::sync::RwLock) or an atomic.
    ///
    /// # Example
    /// ```no_run
    /// use volga::App;
    ///
    /// #[derive(Default, Clone)]
    /// struct Singleton;
    ///
    /// let mut app = App::new();
    /// let singleton = Singleton::default();
    /// app.add_singleton(singleton);
    /// ```
    pub fn add_singleton<T: Send + Sync + 'static>(&mut self, instance: T) -> &mut Self {
        self.container.register_singleton(instance);
        self
    }

    /// Registers scoped service
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, di::{Container, Inject, error::Error}};
    ///
    /// #[derive(Clone)]
    /// struct ScopedService;
    ///
    /// impl Inject for ScopedService {
    ///     fn inject(_: &Container) -> Result<Self, Error> {
    ///         Ok(Self)
    ///     }
    /// }
    ///
    /// let mut app = App::new();
    /// app.add_scoped::<ScopedService>();
    /// ```
    pub fn add_scoped<T: Inject + 'static>(&mut self) -> &mut Self {
        self.container.register_scoped::<T>();
        self
    }

    /// Registers scoped service that required to be resolved via factory
    ///
    /// > **Note:** Provided factory function will be called once per scope
    /// > and the result will be available and reused per this scope lifetime.
    ///
    /// # Example
    /// ```no_run
    /// use volga::App;
    ///
    /// #[derive(Clone)]
    /// struct ScopedService;
    ///
    /// impl ScopedService {
    ///     fn new() -> Self {
    ///         ScopedService
    ///     }
    /// }
    ///
    /// let mut app = App::new();
    /// app.add_scoped_factory(|| ScopedService::new());
    /// ```
    pub fn add_scoped_factory<T, F, Args>(&mut self, factory: F) -> &mut Self
    where
        T: Send + Sync + 'static,
        F: GenericFactory<Args, Output = T>,
        Args: Inject,
    {
        self.container.register_scoped_factory(factory);
        self
    }

    /// Registers scoped service that required to be resolved as [`Default`]
    ///
    /// > **Note:** the [`Default::default`] method will be called once per scope
    /// > and the result will be available and reused per this scope lifetime.
    ///
    /// # Example
    /// ```no_run
    /// use volga::App;
    ///
    /// #[derive(Default, Clone)]
    /// struct ScopedService;
    ///
    /// let mut app = App::new();
    /// app.add_scoped_default::<ScopedService>();
    /// ```
    pub fn add_scoped_default<T>(&mut self) -> &mut Self
    where
        T: Default + Send + Sync + 'static,
    {
        self.container.register_scoped_default::<T>();
        self
    }

    /// Registers transient service
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, di::{Container, Inject, error::Error}};
    ///
    /// #[derive(Clone)]
    /// struct TransientService;
    ///
    /// impl Inject for TransientService {
    ///     fn inject(_: &Container) -> Result<Self, Error> {
    ///         Ok(Self)
    ///     }
    /// }
    ///
    /// let mut app = App::new();
    /// app.add_transient::<TransientService>();
    /// ```
    pub fn add_transient<T: Inject + 'static>(&mut self) -> &mut Self {
        self.container.register_transient::<T>();
        self
    }

    /// Registers transient service that required to be resolved via factory
    ///
    /// > **Note:** Provided factory function will be called
    /// > every time once this service requested.
    ///
    /// # Example
    /// ```no_run
    /// use volga::App;
    ///
    /// #[derive(Clone)]
    /// struct TransientService;
    ///
    /// impl TransientService {
    ///     fn new() -> Self {
    ///         TransientService
    ///     }
    /// }
    ///
    /// let mut app = App::new();
    /// app.add_transient_factory(|| TransientService::new());
    /// ```
    pub fn add_transient_factory<T, F, Args>(&mut self, factory: F) -> &mut Self
    where
        T: Send + Sync + 'static,
        F: GenericFactory<Args, Output = T>,
        Args: Inject,
    {
        self.container.register_transient_factory(factory);
        self
    }

    /// Registers transient service that required to be resolved as [`Default`]
    ///
    /// > **Note:** the [`Default::default`] method will be called
    /// > every time once this service requested.
    ///
    /// # Example
    /// ```no_run
    /// use volga::App;
    ///
    /// #[derive(Default, Clone)]
    /// struct TransientService;
    ///
    /// let mut app = App::new();
    /// app.add_transient_default::<TransientService>();
    /// ```
    pub fn add_transient_default<T>(&mut self) -> &mut Self
    where
        T: Default + Send + Sync + 'static,
    {
        self.container.register_transient_default::<T>();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::App;
    use volga_di::{Container, Inject};

    #[derive(Default)]
    struct TestDependency;

    impl Inject for TestDependency {
        fn inject(_: &Container) -> Result<Self, volga_di::error::Error> {
            Ok(TestDependency)
        }
    }

    #[test]
    fn it_adds_singleton() {
        let mut app = App::new();
        app.add_singleton(TestDependency);

        let container = app.container.build();
        let dep = container.resolve_shared::<TestDependency>();

        assert!(dep.is_ok());
    }

    #[test]
    fn it_adds_scoped() {
        let mut app = App::new();
        app.add_scoped::<TestDependency>();

        let container = app.container.build();
        let dep = container.resolve_shared::<TestDependency>();

        assert!(dep.is_ok());
    }

    #[test]
    fn it_adds_scoped_factory() {
        let mut app = App::new();
        app.add_scoped_factory(|| TestDependency);

        let container = app.container.build();
        let dep = container.resolve_shared::<TestDependency>();

        assert!(dep.is_ok());
    }

    #[test]
    fn it_adds_scoped_default() {
        let mut app = App::new();
        app.add_scoped_default::<TestDependency>();

        let container = app.container.build();
        let dep = container.resolve_shared::<TestDependency>();

        assert!(dep.is_ok());
    }

    #[test]
    fn it_adds_transient() {
        let mut app = App::new();
        app.add_transient::<TestDependency>();

        let container = app.container.build();
        let dep = container.resolve_shared::<TestDependency>();

        assert!(dep.is_ok());
    }

    #[test]
    fn it_adds_transient_factory() {
        let mut app = App::new();
        app.add_transient_factory(|| TestDependency);

        let container = app.container.build();
        let dep = container.resolve_shared::<TestDependency>();

        assert!(dep.is_ok());
    }

    #[test]
    fn it_adds_transient_default() {
        let mut app = App::new();
        app.add_transient_default::<TestDependency>();

        let container = app.container.build();
        let dep = container.resolve_shared::<TestDependency>();

        assert!(dep.is_ok());
    }
}
