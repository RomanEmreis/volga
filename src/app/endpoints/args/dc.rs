﻿//! Extractors for Dependency Injection

use std::io::Error;
use std::ops::{Deref, DerefMut};
use futures_util::future::{ready, Ready};
use crate::app::endpoints::args::{FromPayload, Payload, Source};
use crate::app::di::Inject;

/// `Dc` stands for Dependency Container, This struct wraps the injectable type of `T` 
/// `T` must be registered in Dependency Injection Container
pub struct Dc<T: Inject>(pub T);

impl<T: Inject> Deref for Dc<T> {
    type Target = T;

    fn deref(&self) -> &T {
        &self.0
    }
}

impl<T: Inject> DerefMut for Dc<T> {
    fn deref_mut(&mut self) -> &mut T {
        &mut self.0
    }
}

impl<T: Inject + 'static> FromPayload for Dc<T> {
    type Future = Ready<Result<Self, Error>>;

    fn from_payload(payload: Payload) -> Self::Future {
        if let Payload::Dc(container) = payload {
            let dependency = container
                .resolve::<T>()
                .map(Dc);
            ready(dependency)
        } else {
            unreachable!()
        }
    }

    fn source() -> Source {
        Source::Dc
    }
}