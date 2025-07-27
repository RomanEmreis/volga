//! Tools for dependency injection

pub use crate::{
    container::{Container, ContainerBuilder},
    inject::Inject,
};

#[cfg(feature = "macros")]
pub use volga_macros::Singleton;

pub mod error;
pub mod container;
pub mod inject;