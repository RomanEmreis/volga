//! Volga DI
//! 
//! A standalone, flexible, and easy-to-configure DI container.
//! 
//! # Example
//! ```no_run
//! use std::collections::HashMap;
//! use std::sync::{Arc, Mutex};
//! use volga_di::ContainerBuilder;
//! 
//! #[derive(Default, Clone)]
//! struct InMemoryCache {
//!     inner: Arc<Mutex<HashMap<String, String>>>
//! }
//! 
//! # fn main() {
//! let mut container = ContainerBuilder::new();
//! container.register_singleton(InMemoryCache::default());
//! 
//! let container = container.build();
//! 
//! let Ok(cache) = container.resolve::<InMemoryCache>() else { 
//!     panic!("Unable to resolve InMemoryCache");
//! };
//! # }
//! ```

pub use crate::{
    container::{Container, ContainerBuilder, FromContainer, GenericFactory},
    inject::Inject,
};

pub mod error;
pub mod container;
pub mod inject;