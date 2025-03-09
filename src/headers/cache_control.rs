use std::{fmt, time::SystemTime};
use crate::headers::ETag;

#[cfg(feature = "static-files")]
use crate::error::Error;
#[cfg(feature = "static-files")]
use std::fs::Metadata;

#[cfg(feature = "static-files")]
const DEFAULT_MAX_AGE: u32 = 60 * 60 * 24; // 24 hours

/// Represents the HTTP [`Cache-Control`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Cache-Control)
/// header holds directives (instructions) in both requests and responses that control caching 
/// in browsers and shared caches (e.g., Proxies, CDNs).
#[derive(Debug, Default, Clone, Copy)]
pub struct CacheControl {
    /// The `no-cache` response directive indicates that the response can be stored in caches, 
    /// but the response must be validated with the origin server before each reuse, 
    /// even when the cache is disconnected from the origin server.
    pub no_cache: bool,
    
    /// The `no-store` response directive indicates that any caches of any kind (private or shared)
    /// should not store this response.
    pub no_store: bool,
    
    /// The `max-age` response directive indicates that the response
    /// remains fresh until `N` seconds after the response is generated.
    pub max_age: Option<u32>,
    
    /// The `s-maxage` response directive indicates how long the response remains fresh
    /// in a shared cache. The `s-maxage` directive is ignored by private caches, and overrides 
    /// the value specified by the `max-age` directive or 
    /// the [`Expires`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Expires) header 
    /// for shared caches, if they are present.
    pub s_max_age: Option<u32>,
    
    /// The `must-revalidate` response directive indicates that the response can be stored in caches
    /// and can be reused while fresh. If the response becomes stale, it must be validated 
    /// with the origin server before reuse.
    /// 
    /// Typically, must-revalidate is used with `max-age`.
    pub must_revalidate: bool,
    
    /// The `proxy-revalidate` response directive is the equivalent of `must-revalidate`,
    /// but specifically for shared caches only.
    pub proxy_revalidate: bool,
    
    /// The `public` response directive indicates that the response can be stored in a shared cache.
    /// Responses for requests with 
    /// [`Authorization`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Authorization) header
    /// fields must not be stored in a shared cache; however, the `public` directive will cause such
    /// responses to be stored in a shared cache.
    pub public: bool,
    
    /// The `private` response directive indicates that the response can be stored only 
    /// in a private cache (e.g. local caches in browsers).
    pub private: bool,
    
    /// The `immutable` response directive indicates that the response will not be updated 
    /// while it's fresh.
    pub immutable: bool,
}

impl fmt::Display for CacheControl {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut directives = Vec::new();

        if self.no_cache {
            directives.push("no-cache".to_string());
        }
        if self.no_store {
            directives.push("no-store".to_string());
        }
        if let Some(max_age) = self.max_age {
            directives.push(format!("max-age={}", max_age));
        }
        if let Some(s_max_age) = self.s_max_age {
            directives.push(format!("s-maxage={}", s_max_age));
        }
        if self.must_revalidate {
            directives.push("must-revalidate".to_string());
        }
        if self.proxy_revalidate {
            directives.push("proxy-revalidate".to_string());
        }
        if self.public {
            directives.push("public".to_string());
        }
        if self.private {
            directives.push("private".to_string());
        }
        if self.immutable {
            directives.push("immutable".to_string());
        }
        
        f.write_str(directives.join(", ").as_str())
    }
}

impl From<CacheControl> for String {
    #[inline]
    fn from(cc: CacheControl) -> Self {
        cc.to_string()
    }
}

/// Represents a response caching data that is a composition of:
/// [`ETag`](https://developer.mozilla.org/ru/docs/Web/HTTP/Headers/ETag),
/// [`Last-Modified`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Last-Modified) and
/// [`Cache-Control`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Cache-Control)
pub struct ResponseCaching {
    /// Represents 
    /// [`ETag`](https://developer.mozilla.org/ru/docs/Web/HTTP/Headers/ETag)
    pub(crate) etag: ETag,
    
    /// Represents 
    /// [`Last-Modified`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Last-Modified)
    pub(crate) last_modified: SystemTime,
    
    /// Represents 
    /// [`Cache-Control`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Cache-Control)
    pub(crate) cache_control: CacheControl
}

#[cfg(feature = "static-files")]
impl TryFrom<&Metadata> for ResponseCaching {
    type Error = Error;

    #[inline]
    fn try_from(meta: &Metadata) -> Result<Self, Self::Error> {
        let last_modified = meta.modified()?;
        let etag: ETag = meta.try_into()?;
        let cache_control = CacheControl { 
            public: true, 
            immutable: true, 
            max_age: Some(DEFAULT_MAX_AGE), 
            ..Default::default()
        };
        
        let this = Self {
            cache_control,
            etag,
            last_modified
        };
        Ok(this)
    }
}

impl ResponseCaching {
    /// Returns a [`ETag`](https://developer.mozilla.org/ru/docs/Web/HTTP/Headers/ETag) value as `&str`
    #[inline]
    pub fn etag(&self) -> &str {
        self.etag.as_ref()
    }
    
    /// Returns [`Last-Modified`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Last-Modified)
    /// as [`String`]
    #[inline]
    pub fn last_modified(&self) -> String {
        httpdate::fmt_http_date(self.last_modified)
    }

    /// Returns [`Cache-Control`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Cache-Control)
    /// as [`String`]
    #[inline]
    pub fn cache_control(&self) -> String {
        self.cache_control.into()
    }
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;
    use crate::headers::{CacheControl, ETag, ResponseCaching};

    #[test]
    fn it_creates_cache_control_string() {
        let cache_control = CacheControl {
            max_age: 60.into(),
            public: true,
            must_revalidate: false,
            proxy_revalidate: true,
            no_store: true,
            no_cache: false,
            s_max_age: 60.into(),
            ..Default::default()
        };
        
        assert_eq!("no-store, max-age=60, s-maxage=60, proxy-revalidate, public", cache_control.to_string());
    }
    
    #[test]
    fn if_returns_etag() {
        let caching = ResponseCaching {
          etag: ETag::new("123"), 
          last_modified: SystemTime::now(), 
          cache_control: Default::default()
        };
        
        assert_eq!(caching.etag(), "\"123\"");
    }

    #[test]
    fn if_returns_last_modified_string() {
        let now = SystemTime::now();
        let caching = ResponseCaching {
            etag: ETag::new("123"),
            last_modified: now,
            cache_control: Default::default()
        };

        assert_eq!(caching.last_modified(), httpdate::fmt_http_date(now));
    }

    #[test]
    fn if_returns_cache_control_string() {
        let cache_control = CacheControl {
            max_age: 60.into(),
            private: true,
            immutable: true,
            ..Default::default()
        };
        
        let caching = ResponseCaching {
            etag: ETag::new("123"),
            last_modified: SystemTime::now(),
            cache_control
        };

        assert_eq!(caching.cache_control(), "max-age=60, private, immutable");
    }
}