//! Describes dependency injection errors

use std::fmt::{Display, Formatter};

#[derive(Debug, Clone)]
pub enum Error {
    ContainerMissing,
    Other(&'static str),
    ResolveFailed(&'static str),
    NotRegistered(&'static str)
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self { 
            Error::ContainerMissing => write!(f, "Services Error: DI container is missing"),
            Error::Other(msg) => write!(f, "{msg}"),
            Error::ResolveFailed(type_name) => write!(f, "Services Error: unable to resolve the service: {type_name}"),
            Error::NotRegistered(type_name) => write!(f, "Services Error: service not registered: {type_name}")
        }
    }
}