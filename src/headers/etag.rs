﻿//! Utilities for ETAG header

use std::{
    borrow::Cow,
    fmt::Display,
    ops::Deref
};

#[cfg(feature = "static-files")]
use sha1::{Sha1, Digest};
#[cfg(feature = "static-files")]
use std::fs::Metadata;
#[cfg(feature = "static-files")]
use std::time::UNIX_EPOCH;

/// Represents Entity Tag (ETag) value
#[derive(Debug, Clone)]
pub struct ETag {
    inner: Cow<'static, str>,
}

#[cfg(feature = "static-files")]
impl TryFrom<&Metadata> for ETag {
    type Error = crate::error::Error;
    
    #[inline]
    fn try_from(metadata: &Metadata) -> Result<Self, Self::Error> {
        let mut hasher = Sha1::new();
        hasher.update(metadata.len().to_string());
        
        let mod_time = metadata.modified()?;
        let duration = mod_time.duration_since(UNIX_EPOCH)
            .map_err(Self::Error::server_error)?;

        hasher.update(duration.as_secs().to_string());
        
        Ok(Self::from(format!("\"{:x}\"", hasher.finalize())))
    }
}

impl From<String> for ETag {
    #[inline]
    fn from(s: String) -> Self {
        Self { inner: Cow::Owned(s) }
    }
}

impl Deref for ETag {
    type Target = str;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.inner.as_ref()
    }
}

impl AsRef<str> for ETag {
    #[inline]
    fn as_ref(&self) -> &str {
        self.inner.as_ref()
    }
}

impl Display for ETag {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

impl ETag {
    #[inline]
    pub fn new(etag: &str) -> Self {
        Self::from(format!("\"{etag}\""))
    }
}

#[cfg(test)]
mod tests {
    use crate::headers::ETag;

    #[test]
    fn it_creates_etag() {
        let etag = ETag::new("foo");
        
        assert_eq!(etag.as_ref(), "\"foo\"");
    }

    #[test]
    fn it_creates_etag_from_string() {
        let etag = ETag::from(String::from("\"foo\""));

        assert_eq!(etag.as_ref(), "\"foo\"");
    }

    #[test]
    fn it_creates_string_from_etag() {
        let etag = ETag::new("foo");

        assert_eq!(etag.to_string(), "\"foo\"");
    }

    #[test]
    fn it_compares_etag() {
        let etag1 = ETag::new("foo");
        let etag2 = ETag::new("foo");

        assert_eq!(*etag1, *etag2);
    }
}