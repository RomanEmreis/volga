//! Extractors for ftching data from DI container

use super::{Error, Container};

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
define_generic_from_container! { T1, T2, T3, T4 }
define_generic_from_container! { T1, T2, T3, T4, T5 }

#[cfg(test)]
mod tests {
    use crate::ContainerBuilder;
    use super::*;

    #[derive(Debug, Clone, Copy)]
    struct Dependency {
        x: i32
    }

    impl FromContainer for Dependency {
        fn from_container(container: &Container) -> Result<Self, Error> {
            container.resolve()
        }
    }

    #[test]
    fn it_resolves_from_container() {
        let mut container = ContainerBuilder::new();
        container.register_transient_factory(|| Dependency { x: 1 });

        let container = container.build();

        let dependency = Dependency::from_container(&container).unwrap();

        assert_eq!(dependency.x, 1);
    }

    #[test]
    fn it_resolves_from_container_with_error() {
        let container = ContainerBuilder::new().build();

        let err = Dependency::from_container(&container).unwrap_err();

        assert_eq!(err.to_string(), "Services Error: service not registered: volga_di::container::from_container::tests::Dependency");
    }
}