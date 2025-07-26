//! Utilities for Cache-Control header

use std::{fmt, time::SystemTime};
use crate::{
    App, 
    HttpResponse, HttpResult,
    routing::{Route, RouteGroup}, 
    headers::{HeaderValue, ETag, CACHE_CONTROL}
};

#[cfg(feature = "static-files")]
use crate::error::Error;
#[cfg(feature = "static-files")]
use std::fs::Metadata;
use std::future::Future;

#[cfg(feature = "static-files")]
const DEFAULT_MAX_AGE: u32 = 60 * 60 * 24; // 24 hours

pub const NO_STORE: &str = "no-store";
pub const NO_CACHE: &str = "no-cache";
pub const MAX_AGE: &str = "max-age";
pub const S_MAX_AGE: &str = "s-maxage";
pub const MUST_REVALIDATE: &str = "must-revalidate";
pub const PROXY_REVALIDATE: &str = "proxy-revalidate";
pub const PUBLIC: &str = "public";
pub const PRIVATE: &str = "private";
pub const IMMUTABLE: &str = "immutable"; 

/// Represents the HTTP [`Cache-Control`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Cache-Control)
/// header holds directives (instructions) in both requests and responses that control caching 
/// in browsers and shared caches (e.g., Proxies, CDNs).
#[derive(Debug, Default, Clone, Copy)]
pub struct CacheControl {
    /// The `no-cache` response directive indicates that the response can be stored in caches, 
    /// but the response must be validated with the origin server before each reuse, 
    /// even when the cache is disconnected from the origin server.
    no_cache: bool,
    
    /// The `no-store` response directive indicates that any caches of any kind (private or shared)
    /// should not store this response.
    no_store: bool,
    
    /// The `max-age` response directive indicates that the response
    /// remains fresh until `N` seconds after the response is generated.
    max_age: Option<u32>,
    
    /// The `s-maxage` response directive indicates how long the response remains fresh
    /// in a shared cache. The `s-maxage` directive is ignored by private caches, and overrides 
    /// the value specified by the `max-age` directive or 
    /// the [`Expires`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Expires) header 
    /// for shared caches, if they are present.
    s_max_age: Option<u32>,
    
    /// The `must-revalidate` response directive indicates that the response can be stored in caches
    /// and can be reused while fresh. If the response becomes stale, it must be validated 
    /// with the origin server before reuse.
    /// 
    /// Typically, must-revalidate is used with `max-age`.
    must_revalidate: bool,
    
    /// The `proxy-revalidate` response directive is the equivalent of `must-revalidate`,
    /// but specifically for shared caches only.
    proxy_revalidate: bool,
    
    /// The `public` response directive indicates that the response can be stored in a shared cache.
    /// Responses for requests with 
    /// [`Authorization`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Headers/Authorization) header
    /// fields must not be stored in a shared cache; however, the `public` directive will cause such
    /// responses to be stored in a shared cache.
    public: bool,
    
    /// The `private` response directive indicates that the response can be stored only 
    /// in a private cache (e.g. local caches in browsers).
    private: bool,
    
    /// The `immutable` response directive indicates that the response will not be updated 
    /// while it's fresh.
    immutable: bool,
}

impl fmt::Display for CacheControl {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut directives = Vec::new();

        if self.no_cache {
            directives.push(NO_CACHE.to_string());
        }
        if self.no_store {
            directives.push(NO_STORE.to_string());
        }
        if let Some(max_age) = self.max_age {
            directives.push(format!("{MAX_AGE}={max_age}"));
        }
        if let Some(s_max_age) = self.s_max_age {
            directives.push(format!("{S_MAX_AGE}={s_max_age}"));
        }
        if self.must_revalidate {
            directives.push(MUST_REVALIDATE.to_string());
        }
        if self.proxy_revalidate {
            directives.push(PROXY_REVALIDATE.to_string());
        }
        if self.public {
            directives.push(PUBLIC.to_string());
        }
        if self.private {
            directives.push(PRIVATE.to_string());
        }
        if self.immutable {
            directives.push(IMMUTABLE.to_string());
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

impl TryFrom<CacheControl> for HeaderValue {
    type Error = Error;
    
    #[inline]
    fn try_from(value: CacheControl) -> Result<Self, Self::Error> {
        HeaderValue::from_str(value.to_string().as_str())
            .map_err(Into::into)
    }
}

impl CacheControl {
    /// Enables `no-cache`: forces caches to validate with origin server before reuse.
    /// Disables `immutable`, since they contradict each other.
    pub fn with_no_cache(mut self) -> Self {
        self.no_cache = true;
        self.immutable = false;
        self
    }

    /// Enables `no-store`: disables any form of caching.
    /// Clears `max-age` and `s-maxage`, which would otherwise allow caching.
    pub fn with_no_store(mut self) -> Self {
        self.no_store = true;
        self.max_age = None;
        self.s_max_age = None;
        self
    }

    /// Sets `max-age`: how long the response is fresh in seconds.
    /// Disables `no-store` to allow caching.
    pub fn with_max_age(mut self, max_age: u32) -> Self {
        self.max_age = Some(max_age);
        self.no_store = false;
        self
    }

    /// Sets `s-maxage`: max age for shared (e.g., proxy) caches.
    /// Disables `no-store`.
    pub fn with_s_max_age(mut self, s_max_age: u32) -> Self {
        self.s_max_age = Some(s_max_age);
        self.no_store = false;
        self
    }

    /// Enables `must-revalidate`: once stale, the cache must validate.
    /// Disables `immutable`, which implies no validation.
    pub fn with_must_revalidate(mut self) -> Self {
        self.must_revalidate = true;
        self.immutable = false;
        self
    }

    /// Enables `proxy-revalidate`: like `must-revalidate` but for shared caches.
    /// Disables `immutable`.
    pub fn with_proxy_revalidate(mut self) -> Self {
        self.proxy_revalidate = true;
        self.immutable = false;
        self
    }

    /// Enables `public`: allows shared caches to store response.
    /// Disables `private`, which restricts to client-only.
    pub fn with_public(mut self) -> Self {
        self.public = true;
        self.private = false;
        self
    }

    /// Enables `private`: only client (browser) may cache the response.
    /// Disables `public`.
    pub fn with_private(mut self) -> Self {
        self.private = true;
        self.public = false;
        self
    }

    /// Enables `immutable`: response is guaranteed not to change.
    /// Disables `no-cache`, `must-revalidate`, `proxy-revalidate`.
    pub fn with_immutable(mut self) -> Self {
        self.immutable = true;
        self.no_cache = false;
        self.proxy_revalidate = false;
        self.must_revalidate = false;
        self
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

#[cfg(feature = "middleware")]
impl App {
    /// Adds middleware that includes a configured `cache-control` header for all responses from this server.
    /// 
    /// # Example
    /// ```no_run
    /// use volga::{App, headers::CacheControl};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    /// 
    /// app.use_cache_control(|cache_control| 
    ///     cache_control
    ///         .with_max_age(60)
    ///         .with_immutable()
    ///         .with_public());
    /// 
    /// app.map_get("/hello", || async move { "Hello, World!" });
    /// 
    ///# app.run().await
    ///# }
    /// ```
    pub fn use_cache_control<F>(&mut self, config: F) -> &mut Self
    where 
        F: Fn(CacheControl) -> CacheControl + Clone + Send + Sync + 'static,
    {
        self.map_ok(move |resp: HttpResponse| make_cache_control_fn(resp, config.clone()))
    }
}

#[cfg(feature = "middleware")]
impl<'a> Route<'a> {
    /// Adds middleware that includes a configured `cache-control` header for all responses from this route.
    /// 
    /// # Example
    /// ```no_run
    /// use volga::{App, headers::CacheControl};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    /// 
    /// app.map_get("/hello", || async move { "Hello, World!" })
    ///     .with_cache_control(|cache_control| 
    ///         cache_control
    ///             .with_max_age(60)
    ///             .with_immutable()
    ///             .with_public());
    /// 
    ///# app.run().await
    ///# }
    /// ```
    pub fn with_cache_control<F>(self, config: F) -> Self
    where
        F: Fn(CacheControl) -> CacheControl + Clone + Send + Sync + 'static,
    {
        self.map_ok(move |resp: HttpResponse| make_cache_control_fn(resp, config.clone()))
    }
}

#[cfg(feature = "middleware")]
impl<'a> RouteGroup<'a> {
    /// Adds middleware that includes a configured `cache-control` header for all responses from this group of routes.
    /// 
    /// # Example
    /// ```no_run
    /// use volga::{App, headers::CacheControl};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    /// 
    /// app.map_group("/greeting")
    ///     .with_cache_control(|cache_control| 
    ///         cache_control
    ///             .with_max_age(60)
    ///             .with_immutable()
    ///             .with_public())
    ///     .map_get("/hello", || async move { "Hello, World!" });
    /// 
    ///# app.run().await
    ///# }
    /// ```
    pub fn with_cache_control<F>(self, config: F) -> Self
    where
        F: Fn(CacheControl) -> CacheControl + Clone + Send + Sync + 'static,
    {
        self.map_ok(move |resp: HttpResponse| make_cache_control_fn(resp, config.clone()))
    }
}

#[cfg(feature = "middleware")]
fn make_cache_control_fn<F>(mut resp: HttpResponse, config: F) -> impl Future<Output = HttpResult>
where
    F: Fn(CacheControl) -> CacheControl + Clone + Send + Sync + 'static,
{
    let config = config.clone();
    async move {
        let cache_control = config(CacheControl::default());
        resp.headers_mut()
            .insert(CACHE_CONTROL, cache_control.try_into()?);
        Ok(resp)
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

    #[test]
    fn it_tests_no_store_clears_ages() {
        let cc = CacheControl::default()
            .with_max_age(300)
            .with_s_max_age(120)
            .with_no_store();

        assert!(cc.no_store);
        assert_eq!(cc.max_age, None);
        assert_eq!(cc.s_max_age, None);
    }

    #[test]
    fn it_tests_public_private_conflict() {
        let cc = CacheControl::default().with_private().with_public();
        assert!(cc.public);
        assert!(!cc.private);
    }

    #[test]
    fn it_tests_immutable_conflicts() {
        let cc = CacheControl::default()
            .with_must_revalidate()
            .with_proxy_revalidate()
            .with_no_cache()
            .with_immutable();

        assert!(cc.immutable);
        assert!(!cc.no_cache);
        assert!(!cc.must_revalidate);
        assert!(!cc.proxy_revalidate);
    }

    #[test]
    fn it_tests_max_age_disables_no_store() {
        let cc = CacheControl::default().with_no_store().with_max_age(600);
        assert!(!cc.no_store);
        assert_eq!(cc.max_age, Some(600));
    }

    #[test]
    fn it_tests_combination() {
        let cc = CacheControl::default()
            .with_public()
            .with_max_age(3600)
            .with_immutable();

        assert!(cc.public);
        assert_eq!(cc.max_age, Some(3600));
        assert!(cc.immutable);
    }
}