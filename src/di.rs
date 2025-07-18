﻿//! Tools for Dependency Injection

use super::{App, error::Error};
pub use self::{
    container::{Container, ContainerBuilder},
    dc::Dc,
    inject::Inject
};

pub mod dc;
pub mod inject;
pub mod container;

struct DiError;

impl DiError {
    #[inline]
    fn service_not_registered(type_name: &str) -> Error {
        Error::server_error(format!("Services Error: service not registered: {type_name}"))
    }

    #[inline]
    fn resolve_error(type_name: &str) -> Error {
        Error::server_error(format!("Services Error: unable to resolve the service: {type_name}"))
    }

    #[inline]
    fn container_missing() -> Error {
        Error::server_error("Services Error: DI container is missing")
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
