//! Describes dependency injection errors

use std::fmt::{Display, Formatter};

/// Describes dependency injection error
#[derive(Debug, Clone, Copy)]
pub enum Error {
    /// Indicates that the DI container is missing or not configured
    ContainerMissing,

    /// Indicates that the DI container couldn't resolve a service
    ResolveFailed(&'static str),

    /// Indicates that a requests service has not been registered in the DI container
    NotRegistered(&'static str),

    /// Indicates any other error
    Other(&'static str)
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self { 
            Error::ContainerMissing => write!(f, "Services Error: DI container is missing"),
            Error::ResolveFailed(type_name) => write!(f, "Services Error: unable to resolve the service: {type_name}"),
            Error::NotRegistered(type_name) => write!(f, "Services Error: service not registered: {type_name}"),
            Error::Other(msg) => write!(f, "{msg}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_displays_container_missing() {
        assert_eq!(
            format!("{}", Error::ContainerMissing), 
            "Services Error: DI container is missing"
        );
    }

    #[test]
    fn it_displays_resolve_failed() {
        assert_eq!(
            format!("{}", Error::ResolveFailed("Type")), 
            "Services Error: unable to resolve the service: Type"
        );
    }

    #[test]
    fn it_displays_not_registered() {
        assert_eq!(
            format!("{}", Error::NotRegistered("Type")), 
            "Services Error: service not registered: Type"
        );
    }

    #[test]
    fn it_displays_other() {
        assert_eq!(
            format!("{}", Error::Other("some error")), 
            "some error"
        );
    }
}