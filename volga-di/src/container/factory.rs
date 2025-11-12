//! Generic factory for resolving types

use super::Error;

/// A trait that describes a generic factory function 
/// that can resolve objects registered in DI container
pub trait GenericFactory<Args>: Send + Sync + 'static {
    /// A type of object that will be resolved
    type Output;
    
    /// Calls a generic function and returns either resolved object or error
    fn call(&self, args: Args) -> Result<Self::Output, Error>;
}

impl<F, R> GenericFactory<()> for F
where
    F: Fn() -> R + Send + Sync + 'static
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
        F: Fn($($param),*) -> Result<R, Error> + Send + Sync + 'static,
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
define_generic_factory! { T1 T2 T3 T4 }
define_generic_factory! { T1 T2 T3 T4 T5 }

#[cfg(test)]
mod tests {
    use crate::{Container, ContainerBuilder, Inject};
    use super::*;

    #[derive(Debug, Clone, Copy)]
    struct X(i32);

    #[derive(Debug, Clone, Copy)]
    struct Y(i32);

    #[derive(Debug, Clone, Copy)]
    struct Point(X, Y);

    impl Inject for X {
        fn inject(container: &Container) -> Result<Self, Error> {
            container.resolve()
        }
    }

    impl Inject for Y {
        fn inject(container: &Container) -> Result<Self, Error> {
            container.resolve()
        }
    }

    #[test]
    fn it_resolves_by_injection() {
        let mut container = ContainerBuilder::new();
        container.register_transient_factory(|| X(1));
        container.register_transient_factory(|| Y(2));
        container.register_transient_factory(|x: X, y: Y| Ok(Point(x, y)));

        let container = container.build();

        let point = container.resolve::<Point>().unwrap();
        
        assert_eq!(point.0.0, 1);
        assert_eq!(point.1.0, 2);
    }

        #[test]
    fn it_resolves_from_container() {
        let mut container = ContainerBuilder::new();
        container.register_transient_factory(|| X(1));
        container.register_transient_factory(|| Y(2));
        container.register_transient_factory(|c: Container| {
            let x: X = c.resolve()?;
            let y: Y = c.resolve()?;
            Ok(Point(x, y))
        });

        let container = container.build();

        let point = container.resolve::<Point>().unwrap();
        
        assert_eq!(point.0.0, 1);
        assert_eq!(point.1.0, 2);
    }
}