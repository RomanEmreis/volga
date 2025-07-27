//! Tools for Dependency Injection

use super::{App, error::Error};
pub use {
    self::dc::Dc,
    volga_di::{
        Container, 
        ContainerBuilder,
        Inject, 
        singleton
    },
};

#[cfg(feature = "di-full")]
pub use volga_di::Singleton;

pub mod dc;

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
    pub fn add_singleton<T: Inject + 'static>(&mut self, instance: T) -> &mut Self {
        self.container.register_singleton(instance);
        self
    }

    /// Registers scoped service
    ///
    /// # Example
    /// ```no_run
    /// use volga::App;
    ///
    /// #[derive(Default, Clone)]
    /// struct ScopedService;
    ///
    /// let mut app = App::new();
    /// app.add_scoped::<ScopedService>();
    /// ```
    pub fn add_scoped<T: Inject + 'static>(&mut self) -> &mut Self {
        self.container.register_scoped::<T>();
        self
    }

    /// Registers transient service
    ///
    /// # Example
    /// ```no_run
    /// use volga::App;
    ///
    /// #[derive(Default, Clone)]
    /// struct TransientService;
    ///
    /// let mut app = App::new();
    /// app.add_transient::<TransientService>();
    /// ```
    pub fn add_transient<T: Inject + 'static>(&mut self) -> &mut Self {
        self.container.register_transient::<T>();
        self
    }
}

#[cfg(test)]
mod tests {
    use super::App;
    
    #[derive(Default)]
    struct TestDependency;
    
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
    fn it_adds_transient() {
        let mut app = App::new();
        app.add_transient::<TestDependency>();

        let container = app.container.build();
        let dep = container.resolve_shared::<TestDependency>();

        assert!(dep.is_ok());
    }
}
