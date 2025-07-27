//! Utilities to inject and resolve dependencies

use crate::Container;
use crate::error::Error;

/// A trait that adds the ability to inject dependencies when resolving a type from the DI container
///
/// If there is no need to inject other dependencies, the `struct` must implement the `Default` trait
///
/// # Example
/// ```ignore
/// use volga::{App, di::Dc, ok};
///
/// #[derive(Default, Clone)]
/// struct ScopedService;
///
/// let mut app = App::new();
/// app.add_scoped::<ScopedService>();
///
/// app.map_get("/route", |scoped_service: Dc<ScopedService>| async move {
///     // Do something with scoped service
///     ok!()
/// });
/// ```
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
    fn inject(container: &Container) -> Result<Self, Error>;
}

impl<T: Default + Send + Sync> Inject for T {
    #[inline]
    fn inject(_: &Container) -> Result<Self, Error> {
        Ok(Self::default())
    }
}

/// A `singleton!` macro that simplifies implementing the [`Inject`] trait for one or more singleton types.
///
/// # Macro Syntax
/// ```ignore
/// singleton! {
///     Type1 
///     Type2 
///     …
///     TypeN
/// };
/// ```
/// Each `Type` corresponds to a type for which the `Inject` trait implementation 
/// will be provided. The macro allows specifying one or more types.
///
/// # Example
/// ```ignore
/// use volga::di::{singleton, Error, Inject, Container};
///
/// struct MyType;
/// struct AnotherType;
///
/// singleton! { 
///     MyType
///     AnotherType
/// };
///
/// // Now `MyType` and `AnotherType` have `Inject` implementations:
/// 
/// let mut container = ContainerBuilder::new();
/// container.register_singleton::<MyType>();
/// container.register_singleton::<AnotherType>();
/// 
/// let container = container.build();
/// let result: Result<MyType, Error> = MyType::inject(&container);
/// 
/// assert!(matches!(result, Err(Error::ResolveFailed("MyType"))));
/// ```
///
/// # Behavior
/// - `inject` will always return `Err(Error::ResolveFailed)`, where the error's 
///   message is the name of the type that could not be resolved, as a string.
///
/// # Errors
/// - When attempting to use the `inject` function, the macro-generated implementation 
///   always produces an error of type `Error::ResolveFailed` because it does not 
///   define a mechanism for successfully resolving the type.
#[macro_export]
macro_rules! singleton {
    ($($name:ident)*) => {
        $(impl $crate::Inject for $name {
            #[inline]
            fn inject(_: &$crate::Container) -> Result<Self, $crate::error::Error> {
                Err($crate::error::Error::ResolveFailed(stringify!($name)))
            }
        })*      
    };
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::container::ContainerBuilder;
    use std::sync::{Arc, Mutex};

    #[derive(Default, Clone)]
    struct SimpleService {
        value: i32,
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
    
    #[derive(Debug)]
    struct SingletonService;
    
    singleton! {
        SingletonService
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
    fn it_injects_service_with_dependencies() {
        let mut builder = ContainerBuilder::new();
        builder.register_scoped::<SimpleService>();
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
    fn it_prevents_singleton_injection() {
        let container = ContainerBuilder::new().build();

        let result = SingletonService::inject(&container);

        assert!(result.is_err());
        match result.unwrap_err() {
            Error::ResolveFailed(msg) => assert_eq!(msg, "SingletonService"),
            _ => panic!("Expected ResolveFailed error with singleton message"),
        }
    }


    #[test]
    fn it_tests_send_sync_requirements() {
        fn assert_send_sync<T: Send + Sync>() {}

        // These should compile without issues due to the Send + Sync bounds
        assert_send_sync::<SimpleService>();
        assert_send_sync::<ServiceWithDependency>();
        assert_send_sync::<ComplexService>();
    }
}

