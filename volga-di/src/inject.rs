//! Utilities to inject and resolve dependencies

use crate::Container;
use crate::error::Error;
use std::any::TypeId;

/// A trait that adds the ability to inject dependencies when resolving a type from the DI container
///
/// If it's required to construct a `struct` from other dependencies, the `Inject` can be implemented manually
///
/// # Example
/// ```ignore
/// use volga::{
///     App,
///     error::Error,
///     di::{Dc, Inject, Container},
///     ok
/// };
///
/// #[derive(Default, Clone)]
/// struct ScopedService;
///
/// #[derive(Clone)]
/// struct TransientService {
///     service: ScopedService
/// }
///
/// impl Inject for TransientService {
///     fn inject(container: &Container) -> Result<Self, Error> {
///         let scoped_service = container
///             .resolve::<ScopedService>()?;
///         Ok(Self { service: scoped_service })
///     }
/// }
///
/// let mut app = App::new();
/// app.add_scoped::<ScopedService>();
/// app.add_transient::<TransientService>();
///
/// app.map_get("/route", |transient_service: Dc<TransientService>| async move {
///     let scoped = &transient_service.service;
///     // Do something with scoped and/or transient service
///     ok!()
/// });
/// ```
pub trait Inject: Sized + Send + Sync {
    /// Constructs a type with dependencies
    fn inject(container: &Container) -> Result<Self, Error>;

    /// Declares the services [`inject`](Self::inject) resolves from the container.
    ///
    /// [`ContainerBuilder::validate`](crate::ContainerBuilder::validate) reads these to
    /// check the dependency graph before anything is resolved, so a cycle or a dependency
    /// nobody registered is reported when the container is validated rather than when a
    /// request first reaches it. The default declares nothing: that keeps the type out of
    /// the check without making it wrong, and a cycle through it is still stopped when it
    /// is resolved (see [`Container::resolve_shared`]).
    ///
    /// Declare exactly what `inject` resolves - a dependency declared here but never
    /// resolved there is checked all the same.
    ///
    /// # Example
    /// ```
    /// use volga_di::{Container, ContainerBuilder, Dependencies, Inject, error::Error};
    ///
    /// #[derive(Default, Clone)]
    /// struct Clock;
    ///
    /// struct Session {
    ///     clock: Clock,
    /// }
    ///
    /// impl Inject for Session {
    ///     fn inject(container: &Container) -> Result<Self, Error> {
    ///         Ok(Self { clock: container.resolve::<Clock>()? })
    ///     }
    ///
    ///     fn dependencies(deps: &mut Dependencies) {
    ///         deps.add::<Clock>();
    ///     }
    /// }
    ///
    /// let mut builder = ContainerBuilder::new();
    /// builder.register_scoped::<Session>();
    ///
    /// // `Clock` is declared but not registered
    /// assert!(builder.validate().is_err());
    /// ```
    #[inline]
    fn dependencies(_deps: &mut Dependencies) {}
}

/// The services a type resolves when it is injected, as declared by
/// [`Inject::dependencies`]
#[derive(Debug)]
pub struct Dependencies {
    list: Vec<Dependency>,
}

/// One declared dependency: the service's type, and its name for reports
#[derive(Debug, Clone, Copy)]
pub(crate) struct Dependency {
    pub(crate) id: TypeId,
    pub(crate) name: &'static str,
}

impl Dependencies {
    /// Declares that injecting this type resolves `T` from the container
    #[inline]
    pub fn add<T: Send + Sync + 'static>(&mut self) {
        self.list.push(Dependency {
            id: TypeId::of::<T>(),
            name: std::any::type_name::<T>(),
        });
    }

    /// Collects what `T` declares
    #[inline]
    pub(crate) fn of<T: Inject>() -> Vec<Dependency> {
        let mut deps = Self { list: Vec::new() };
        T::dependencies(&mut deps);
        deps.list
    }
}

// Handing out the container itself is the way around declaring anything: what is resolved
// through it cannot be known here, so it declares nothing and is left to the check made
// while resolving
impl Inject for Container {
    #[inline]
    fn inject(container: &Container) -> Result<Self, Error> {
        Ok(container.clone())
    }
}

impl Inject for () {
    #[inline]
    fn inject(_: &Container) -> Result<Self, Error> {
        Ok(())
    }
}

macro_rules! define_inject {
    ($($T: ident),*) => {
        impl<$($T: Inject),+> Inject for ($($T,)+) {
            #[inline]
            #[allow(non_snake_case)]
            fn inject(container: &Container) -> Result<Self, Error> {
                let tuple = (
                    $(
                    $T::inject(container)?,
                    )*
                );
                Ok(tuple)
            }

            #[inline]
            fn dependencies(deps: &mut Dependencies) {
                $( $T::dependencies(deps); )*
            }
        }
    }
}

define_inject! { T1 }
define_inject! { T1, T2 }
define_inject! { T1, T2, T3 }
define_inject! { T1, T2, T3, T4 }
define_inject! { T1, T2, T3, T4, T5 }

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::ContainerBuilder;
    use std::sync::{Arc, Mutex};

    #[derive(Default, Clone)]
    struct SimpleService {
        value: i32,
    }

    impl Inject for SimpleService {
        fn inject(_: &Container) -> Result<Self, Error> {
            Ok(Default::default())
        }
    }

    #[derive(Clone)]
    struct ServiceWithDependency {
        service: SimpleService,
        multiplier: i32,
    }

    impl Inject for ServiceWithDependency {
        fn inject(container: &Container) -> Result<Self, Error> {
            let service = container.resolve::<SimpleService>()?;
            Ok(Self {
                service,
                multiplier: 2,
            })
        }
    }

    #[derive(Clone)]
    struct ComplexService {
        dependency: ServiceWithDependency,
        data: Arc<Mutex<Vec<String>>>,
    }

    impl Inject for ComplexService {
        fn inject(container: &Container) -> Result<Self, Error> {
            let dependency = container.resolve::<ServiceWithDependency>()?;
            Ok(Self {
                dependency,
                data: Arc::new(Mutex::new(vec!["test".to_string()])),
            })
        }
    }

    #[derive(Debug)]
    struct FailingService;

    impl Inject for FailingService {
        fn inject(_: &Container) -> Result<Self, Error> {
            Err(Error::Other("Injection failed"))
        }
    }

    #[test]
    fn it_injects_default_service() {
        let container = ContainerBuilder::new().build();

        let result = SimpleService::inject(&container);

        assert!(result.is_ok());
        let service = result.unwrap();
        assert_eq!(service.value, 0);
    }

    #[test]
    #[allow(clippy::redundant_closure)]
    fn it_injects_service_with_dependencies() {
        let mut builder = ContainerBuilder::new();
        builder.register_scoped_factory(|c: Container| SimpleService::inject(&c));
        let container = builder.build().create_scope();

        let result = ServiceWithDependency::inject(&container);

        assert!(result.is_ok());
        let service = result.unwrap();
        assert_eq!(service.service.value, 0);
        assert_eq!(service.multiplier, 2);
    }

    #[test]
    fn it_injects_complex_service_with_nested_dependencies() {
        let mut builder = ContainerBuilder::new();
        builder.register_scoped::<SimpleService>();
        builder.register_scoped::<ServiceWithDependency>();
        let container = builder.build().create_scope();

        let result = ComplexService::inject(&container);

        assert!(result.is_ok());
        let service = result.unwrap();
        assert_eq!(service.dependency.service.value, 0);
        assert_eq!(service.dependency.multiplier, 2);
        let data = service.data.lock().unwrap();
        assert_eq!(data[0], "test");
    }

    #[test]
    fn it_fails_when_dependency_not_registered() {
        let container = ContainerBuilder::new().build();

        let result = ServiceWithDependency::inject(&container);

        assert!(result.is_err());
    }

    #[test]
    fn it_handles_injection_errors() {
        let container = ContainerBuilder::new().build();

        let result = FailingService::inject(&container);

        assert!(result.is_err());
        match result.unwrap_err() {
            Error::Other(msg) => assert_eq!(msg, "Injection failed"),
            _ => panic!("Expected Other error"),
        }
    }

    #[test]
    fn it_uses_default_trait_implementation_for_inject() {
        let container = ContainerBuilder::new().build();

        // Test that the blanket implementation works for Default types
        let result = <SimpleService as Inject>::inject(&container);

        assert!(result.is_ok());
        let service = result.unwrap();
        assert_eq!(service.value, 0);
    }

    #[test]
    fn it_resolves_same_dependency_multiple_times() {
        let mut builder = ContainerBuilder::new();
        builder.register_scoped::<SimpleService>();
        let container = builder.build().create_scope();

        let result1 = ServiceWithDependency::inject(&container);
        let result2 = ServiceWithDependency::inject(&container);

        assert!(result1.is_ok());
        assert!(result2.is_ok());

        let service1 = result1.unwrap();
        let service2 = result2.unwrap();

        // Both should have the same underlying service instance (scoped)
        assert_eq!(service1.service.value, service2.service.value);
        assert_eq!(service1.multiplier, service2.multiplier);
    }

    #[test]
    fn it_works_with_different_service_lifetimes() {
        let mut builder = ContainerBuilder::new();
        builder.register_singleton(SimpleService { value: 100 });
        builder.register_transient::<ServiceWithDependency>();
        let container = builder.build();

        let result1 = ServiceWithDependency::inject(&container);
        let result2 = ServiceWithDependency::inject(&container);

        assert!(result1.is_ok());
        assert!(result2.is_ok());

        let service1 = result1.unwrap();
        let service2 = result2.unwrap();

        // Singleton dependency should be the same
        assert_eq!(service1.service.value, 100);
        assert_eq!(service2.service.value, 100);
    }

    #[test]
    fn it_tests_send_sync_requirements() {
        fn assert_send_sync<T: Send + Sync>() {}

        // These should compile without issues due to the Send and Sync bounds
        assert_send_sync::<SimpleService>();
        assert_send_sync::<ServiceWithDependency>();
        assert_send_sync::<ComplexService>();
    }
}
