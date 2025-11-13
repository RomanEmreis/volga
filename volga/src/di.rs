//! Tools for Dependency Injection

use super::{App, error::Error};
pub use {
    self::dc::Dc,
    volga_di::{
        Container, 
        ContainerBuilder,
        GenericFactory,
        Inject
    },
};

pub mod dc;

/// Dependency injection errors
pub mod error {
    pub use volga_di::error::Error;
}

impl From<error::Error> for Error {
    #[inline]
    fn from(err: error::Error) -> Self {
        Error::server_error(err.to_string())
    }
}

/// DI specific impl for [`App`]
impl App {
    /// Registers singleton service
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
        Args: Inject
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
        Args: Inject
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
    use volga_di::{Container, Inject};
    use super::App;
    
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
