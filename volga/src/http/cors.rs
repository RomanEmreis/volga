//! CORS (Cross-Origin Resource Sharing) configuration

use crate::{App, routing::{Route, RouteGroup}};
use hyper::{
    http::{HeaderValue, HeaderName, HeaderMap},
    header::{ORIGIN, ACCESS_CONTROL_REQUEST_METHOD, ACCESS_CONTROL_REQUEST_HEADERS},
    Method,
};

use std::{
    sync::Arc,
    collections::{HashSet, HashMap},
    str::FromStr,
    time::Duration
};

use crate::headers::{
    ACCESS_CONTROL_ALLOW_CREDENTIALS,
    ACCESS_CONTROL_ALLOW_HEADERS,
    ACCESS_CONTROL_ALLOW_METHODS,
    ACCESS_CONTROL_ALLOW_ORIGIN,
    ACCESS_CONTROL_EXPOSE_HEADERS,
    ACCESS_CONTROL_MAX_AGE,
    CONTENT_LENGTH,
    VARY
};

const DEFAULT_MAX_AGE: u64 = 24 * 60 * 60; // 24 hours = 86,400 seconds
const SEPARATOR: &str = ", ";
const WILDCARD_STR: &str = "*";
const ORIGIN_STR: &str = "Origin";
const WILDCARD_VALUE: HeaderValue = HeaderValue::from_static(WILDCARD_STR);
const TRUE_VALUE: HeaderValue = HeaderValue::from_static("true");

const DEFAULT_PREFLIGHT_HEADERS: [HeaderName; 3] = [
    ORIGIN,
    ACCESS_CONTROL_REQUEST_METHOD,
    ACCESS_CONTROL_REQUEST_HEADERS,
];

/// Represents the CORS (Cross-Origin Resource Sharing) Middleware configuration options
#[derive(Debug, Clone)]
pub struct CorsConfig {
    name: Option<String>,
    allow_origins: Option<HashSet<HeaderValue>>,
    allow_headers: Option<HashSet<HeaderName>>,
    allow_methods: Option<HashSet<Method>>,
    expose_headers: Option<HashSet<HeaderName>>,
    expose_any: bool,
    max_age: Option<Duration>,
    allow_credentials: bool,
    include_vary: bool
}

/// represents pre-computed CORS headers
#[derive(Debug)]
pub(crate) struct CorsHeaders {
    allow_origins: Option<HashSet<HeaderValue>>,
    allow_any_origin: bool,
    allow_credentials: bool,
    vary_preflight: Option<HeaderValue>,
    vary_normal: Option<HeaderValue>,
    common: HeaderMap,
    preflight: HeaderMap,
    normal: HeaderMap,
}

/// Represents a set of CORS policies, including the default one
#[derive(Debug, Default)]
pub(crate) struct CorsRegistry {
    default: Option<Arc<CorsHeaders>>,
    named: HashMap<Arc<str>, Arc<CorsHeaders>>,
    pub(crate) is_enabled: bool,
}

/// Describes how CORS bound to a route 
#[derive(Debug, Default, Clone)]
pub(crate) enum CorsOverride {
    #[default]
    Inherit,
    Disabled,
    Named(Arc<CorsHeaders>),
}

impl Default for CorsConfig {
    #[inline]
    fn default() -> Self {
        Self {
            max_age: Some(Duration::from_secs(DEFAULT_MAX_AGE)),
            allow_credentials: false,
            include_vary: true,
            expose_any: false,
            expose_headers: None,
            allow_origins: None,
            allow_headers: None,
            allow_methods: None,
            name: None,
        }
    }
}

impl CorsConfig {
    /// Specifies optional CORS Policy name
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Configures CORS with allowed origins, 
    /// which will be used with the [`Access-Control-Allow-Origin`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Access-Control-Allow-Origin) HTTP header
    ///
    /// Default value: `None` (Any Origin is allowed)
    ///
    /// # Example
    /// ```no_run
    /// use volga::http::CorsConfig;
    ///
    /// let config = CorsConfig::default()
    ///     .with_origins(["http://example.com", "https://example.net"]);
    /// ```
    pub fn with_origins<T, S>(mut self, origins: T) -> Self
    where
        T: IntoIterator<Item = S>,
        S: AsRef<str>
    {
        let allowed_origins = origins
            .into_iter()
            .map(|o| HeaderValue::from_str(o.as_ref())
                .expect("CORS error: invalid origin value"))
            .collect::<HashSet<_>>();
        self.allow_origins = Some(allowed_origins);
        self
    }

    /// Configures CORS to allow any origin 
    ///
    /// Default value: `None` (Any Origin is allowed)
    /// 
    /// # Example
    /// ```no_run
    /// use volga::http::CorsConfig;
    ///
    /// let config = CorsConfig::default()
    ///     .with_any_origin();
    /// ```
    pub fn with_any_origin(mut self) -> Self {
        self.allow_origins = None;
        self
    }
    
    /// Configures CORS with allowed HTTP headers list 
    /// which will be used with the [`Access-Control-Allow-Headers`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Access-Control-Allow-Headers) HTTP header
    /// 
    /// Default value: `None` (Any HTTP header is allowed)
    /// 
    /// # Example
    /// ```no_run
    /// use volga::http::CorsConfig;
    ///
    /// let config = CorsConfig::default()
    ///     .with_headers(["Content-Type", "X-Req-Id"]);
    /// ```
    pub fn with_headers<T, S>(mut self, headers: T) -> Self
    where
        T: IntoIterator<Item = S>,
        S: AsRef<str>
    {
        let allowed_headers = headers
            .into_iter()
            .map(|h| HeaderName::from_str(h.as_ref())
                .expect("CORS error: invalid header value"))
            .collect::<HashSet<_>>();
        self.allow_headers = Some(allowed_headers);
        self
    }

    /// Configures CORS to allow any HTTP header 
    ///
    /// Default value: `None` (Any HTTP header is allowed)
    /// 
    /// # Example
    /// ```no_run
    /// use volga::http::CorsConfig;
    ///
    /// let config = CorsConfig::default()
    ///     .with_any_header();
    /// ```
    pub fn with_any_header(mut self) -> Self {
        self.allow_headers = None;
        self
    }

    /// Configures CORS with allowed HTTP methods 
    /// which will be used with the [`Access-Control-Allow-Methods`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Access-Control-Allow-Methods) HTTP header
    ///
    /// Default value: `None` (Any HTTP method is allowed)
    ///
    /// # Example
    /// ```no_run
    /// use volga::http::{CorsConfig, Method};
    ///
    /// let config = CorsConfig::default()
    ///     .with_methods([Method::GET, Method::PUT, Method::POST]);
    /// ```
    pub fn with_methods<T, I>(mut self, methods: T) -> Self
    where
        T: IntoIterator<Item = I>,
        I: TryInto<Method>,
        <I as TryInto<Method>>::Error: std::fmt::Debug,
    {
        let allowed_methods = methods
            .into_iter()
            .map(|m| m.try_into().expect("valid HTTP method"))
            .collect::<HashSet<_>>();
        self.allow_methods = Some(allowed_methods);
        self
    }

    /// Configures CORS to allow any HTTP method 
    ///
    /// Default value: `None` (Any HTTP method is allowed)
    /// 
    /// # Example
    /// ```no_run
    /// use volga::http::CorsConfig;
    ///
    /// let config = CorsConfig::default()
    ///     .with_any_method();
    /// ```
    pub fn with_any_method(mut self) -> Self {
        self.allow_methods = None;
        self
    }

    /// Configures CORS with `max-age` value in seconds.
    /// Which will be used with the [`Access-Control-Max-Age`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Access-Control-Max-Age) HTTP header.
    /// 
    /// Default value: 86,400 seconds (24 hours)
    ///
    /// # Example
    /// ```no_run
    /// use volga::http::CorsConfig;
    ///
    /// let config = CorsConfig::default()
    ///     .with_max_age(10);
    /// ```
    pub fn with_max_age(mut self, secs: u64) -> Self {
        self.max_age = Some(Duration::from_secs(secs));
        self
    }

    /// Configures CORS to disable [`Access-Control-Max-Age`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Access-Control-Max-Age) header
    ///
    /// Default value: 86,400 seconds (24 hours)
    /// 
    /// # Example
    /// ```no_run
    /// use volga::http::CorsConfig;
    ///
    /// let config = CorsConfig::default()
    ///     .with_any_method();
    /// ```
    pub fn without_max_age(mut self) -> Self {
        self.max_age = None;
        self
    }

    /// Configures CORS whether allow credentials.
    ///
    /// Default value: `false`
    ///
    /// # Example
    /// ```no_run
    /// use volga::http::CorsConfig;
    ///
    /// let config = CorsConfig::default()
    ///     .with_credentials(true);
    /// ```
    pub fn with_credentials(mut self, allow: bool) -> Self {
        self.allow_credentials = allow;
        self
    }

    /// Configures CORS whether include a `Vary` HTTP header
    ///
    /// Default value: `true`
    ///
    /// # Example
    /// ```no_run
    /// use volga::http::CorsConfig;
    ///
    /// let config = CorsConfig::default()
    ///     .with_vary_header(false);
    /// ```
    pub fn with_vary_header(mut self, include_vary: bool) -> Self {
        self.include_vary = include_vary;
        self
    }

    /// Configures CORS with HTTP headers to expose
    /// which will be used with the [`Access-Control-Expose-Headers`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Access-Control-Expose-Headers) HTTP header
    ///
    /// Default value: `None` (Any HTTP header is allowed)
    ///
    /// # Example
    /// ```no_run
    /// use volga::http::CorsConfig;
    ///
    /// let config = CorsConfig::default()
    ///     .with_expose_headers(["Content-Type", "X-Req-Id"]);
    /// ```
    pub fn with_expose_headers<T, S>(mut self, headers: T) -> Self
    where
        T: IntoIterator<Item = S>,
        S: AsRef<str>
    {
        let allowed_headers = headers
            .into_iter()
            .map(|h| HeaderName::from_str(h.as_ref())
                .expect("CORS error: invalid header value"))
            .collect::<HashSet<_>>();
        self.expose_headers = Some(allowed_headers);
        self
    }

    /// Configures CORS to allow any HTTP header to expose
    ///
    /// Default value: `None` (Any HTTP header is allowed)
    ///
    /// # Example
    /// ```no_run
    /// use volga::http::CorsConfig;
    ///
    /// let config = CorsConfig::default()
    ///     .with_expose_any_header();
    /// ```
    pub fn with_expose_any_header(mut self) -> Self {
        self.expose_any = true;
        self.expose_headers = None;
        self
    }
    
    /// Creates a value for the [`Access-Control-Allow-Methods`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Access-Control-Allow-Methods)
    /// HTTP header
    pub fn allow_methods(&self) -> Option<HeaderValue> {
        match &self.allow_methods { 
            None => Some(WILDCARD_VALUE),
            Some(allow_methods) => {
                let allow_methods = allow_methods
                    .iter()
                    .map(|method| method.as_str())
                    .collect::<Vec<_>>().join(", ");
                HeaderValue::from_str(&allow_methods).ok()
            }
        }
    }

    /// Creates a value for the [`Access-Control-Allow-Headers`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Access-Control-Allow-Headers)
    /// HTTP header
    pub fn allow_headers(&self) -> Option<HeaderValue> {
        match &self.allow_headers { 
            None => Some(WILDCARD_VALUE),
            Some(allow_headers) => {
                let allow_headers = build_csv(
                    allow_headers
                        .iter()
                        .map(|h| h.as_str())
                );
                HeaderValue::from_str(&allow_headers).ok()
            }
        }
    }

    /// Creates a value for the [`Access-Control-Expose-Headers`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Access-Control-Expose-Headers)
    /// HTTP header
    pub fn expose_headers(&self) -> Option<HeaderValue> {
        match &self.expose_headers {
            None if self.expose_any => Some(WILDCARD_VALUE),
            None => None,
            Some(expose_headers) => {
                let expose_headers = build_csv(
                    expose_headers
                        .iter()
                        .map(|h| h.as_str())
                );
                HeaderValue::from_str(&expose_headers).ok()
            }
        }
    }

    /// Creates a value for the [`Access-Control-Max-Age`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Access-Control-Max-Age)
    /// HTTP header
    pub fn max_age(&self) -> Option<HeaderValue> {
        match &self.max_age { 
            None => None,
            Some(max_age) => HeaderValue::from_str(
                itoa::Buffer::new().format(max_age.as_secs())
            ).ok()
        }
    }

    /// Creates a value for the [`Access-Control-Allow-Credentials`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Access-Control-Allow-Credentials)
    /// HTTP header
    pub fn allow_credentials(&self) -> Option<HeaderValue> {
        if self.allow_credentials {
            Some(TRUE_VALUE)
        } else {
            None
        }
    }

    /// Creates a value for the `Vary` HTTP header for preflight requests
    pub fn vary_preflight(&self) -> Option<HeaderValue> {
        if self.needs_vary() {
            let vary_header = DEFAULT_PREFLIGHT_HEADERS.join(SEPARATOR);
            HeaderValue::from_str(&vary_header).ok()
        } else {
            None
        }
    }

    /// Creates a value for the `Vary` HTTP header for normal requests
    pub fn vary_normal(&self) -> Option<HeaderValue> {
        if self.needs_vary() {
            Some(HeaderValue::from_static(ORIGIN_STR))
        } else {
            None
        }
    }

    /// Pre-compute CORS headers
    pub(crate) fn precompute(self) -> CorsHeaders {
        let mut common = HeaderMap::new();
        if let Some(v) = self.allow_credentials() {
            common.insert(ACCESS_CONTROL_ALLOW_CREDENTIALS, v);
        }

        let mut preflight = HeaderMap::new();
        if let Some(v) = self.allow_methods() {
            preflight.insert(ACCESS_CONTROL_ALLOW_METHODS, v);
        }
        if let Some(v) = self.allow_headers() {
            preflight.insert(ACCESS_CONTROL_ALLOW_HEADERS, v);
        }
        if let Some(v) = self.max_age() {
            preflight.insert(ACCESS_CONTROL_MAX_AGE, v);
        }
        preflight.insert(CONTENT_LENGTH, HeaderValue::from_static("0"));

        let mut normal = HeaderMap::new();
        if let Some(v) = self.expose_headers() {
            normal.insert(ACCESS_CONTROL_EXPOSE_HEADERS, v);
        }

        CorsHeaders {
            allow_any_origin: self.allow_origins.is_none(),
            vary_normal: self.vary_normal(),
            vary_preflight: self.vary_preflight(),
            allow_origins: self.allow_origins,
            allow_credentials: self.allow_credentials,
            common,
            preflight,
            normal,
        }
    }
    
    /// Validates the [`CorsConfig`] and panics if it's invalid
    pub(crate) fn validate(self) -> Self {
        if self.allow_credentials {
            assert!(
                self.allow_origins.is_some(),
                "CORS error: The `Access-Control-Allow-Credentials: true` cannot be used \
                with `Access-Control-Allow-Origin: *`"
            );

            assert!(
                self.allow_headers.is_some(),
                "CORS error: The `Access-Control-Allow-Credentials: true` cannot be used \
                with `Access-Control-Allow-Headers: *`"
            );

            assert!(
                self.allow_methods.is_some(),
                "CORS error: The `Access-Control-Allow-Credentials: true` cannot be used \
                with `Access-Control-Allow-Methods: *`"
            );

            assert!(
                !self.expose_any,
                "CORS error: The `Access-Control-Allow-Credentials: true` cannot be used \
                with `Access-Control-Expose-Headers: *`"
            );
        }

        self
    }

    #[inline(always)]
    fn needs_vary(&self) -> bool {
        (self.allow_credentials || self.allow_origins.is_some()) && self.include_vary
    } 
}

impl CorsHeaders {
    /// Creates a value for the [`Access-Control-Allow-Origin`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Access-Control-Allow-Origin)
    /// HTTP header
    #[inline]
    pub(crate) fn allow_origin(&self, origin: Option<HeaderValue>) -> Option<HeaderValue> {
        match (self.allow_any_origin, self.allow_credentials) {
            (true, false) => Some(WILDCARD_VALUE),
            (true, true) => None,
            (false, _) => {
                let o = origin?;
                let set = self.allow_origins.as_ref()?;
                if set.contains(&o) { Some(o) } else { None }
            }
        }
    }

    #[inline]
    pub(crate) fn apply_preflight_response(
        &self,
        headers: &mut HeaderMap,
        origin: Option<HeaderValue>,
    ) {
        self.apply_common(headers, origin);

        if let Some(v) = &self.vary_preflight {
            headers.append(VARY, v.clone());
        }

        Self::apply_headers(headers, &self.preflight);
    }

    #[inline]
    pub(crate) fn apply_normal_response(
        &self,
        headers: &mut HeaderMap,
        origin: Option<HeaderValue>,
    ) {
        self.apply_common(headers, origin);

        if let Some(v) = &self.vary_normal {
            Self::merge_vary_origin(headers, v.clone());
        }

        Self::apply_headers(headers, &self.normal);
    }

    #[inline]
    fn apply_common(&self, headers: &mut HeaderMap, origin: Option<HeaderValue>) {
        if let Some(ao) = self.allow_origin(origin) {
            headers.insert(ACCESS_CONTROL_ALLOW_ORIGIN, ao);
        }

        Self::apply_headers(headers, &self.common);
    }

    #[inline]
    fn apply_headers(dst: &mut HeaderMap, src: &HeaderMap) {
        src.iter().for_each(|(k, v)| {
            dst.insert(k, v.clone());
        });
    }

    #[inline]
    fn merge_vary_origin(headers: &mut HeaderMap, vary: HeaderValue) {
        match headers.get(VARY) {
            None => {
                headers.insert(VARY, vary);
            }
            Some(existing) => {
                let Ok(s) = existing.to_str() else {
                    return;
                };

                if s.trim() == WILDCARD_STR {
                    return;
                }

                let already_has_origin = s
                    .split(',')
                    .map(|p| p.trim())
                    .any(|p| p.eq_ignore_ascii_case(ORIGIN_STR));

                if already_has_origin {
                    return;
                }

                let mut merged = String::with_capacity(s.len() + 2 + ORIGIN_STR.len());
                
                merged.push_str(s);
                merged.push_str(SEPARATOR);
                merged.push_str(ORIGIN_STR);

                if let Ok(v) = HeaderValue::from_str(&merged) {
                    headers.insert(VARY, v);
                }
            }
        }
    }
}

impl CorsRegistry {
    /// Returns `true` if CORS policies are registered
    #[inline]
    pub(crate) fn registered(&self) -> bool {
        self.default.is_some() || !self.named.is_empty()
    }

    /// Sets the default CORS policy
    #[inline]
    pub(crate) fn set_default(&mut self, cfg: CorsConfig) {
        self.default = Some(Arc::new(cfg.validate().precompute()));
    }

    /// Inserts a named CORS policy
    #[inline]
    pub(crate) fn insert_named(&mut self, name: impl Into<Arc<str>>, cfg: CorsConfig) {
        self.named.insert(name.into(), Arc::new(cfg.validate().precompute()));
    }

    /// Returns CORS policy by name
    #[inline]
    pub(crate) fn get_named(&self, name: &str) -> Option<&Arc<CorsHeaders>> {
        self.named.get(name)
    }

    /// Returns default CORS policy
    #[inline]
    pub(crate) fn get_default(&self) -> Option<&Arc<CorsHeaders>> {
        self.default.as_ref()
    }
}

impl App {
    /// Configures a web server with specified CORS configuration
    ///
    /// Default: `None`
    ///
    /// # Example
    /// ```no_run
    /// use volga::App;
    ///
    /// let app = App::new()
    ///     .with_cors(|cors| cors.with_any_origin());
    /// ```
    ///
    /// If CORS was already preconfigured, it does not overwrite it
    /// ```no_run
    /// use volga::App;
    /// use volga::http::CorsConfig;
    ///
    /// let app = App::new()
    ///     .set_cors(CorsConfig::default().with_any_origin())
    ///     .with_cors(|cors| cors
    ///         .with_any_method()
    ///         .with_any_header());
    /// ```
    pub fn with_cors<T>(self, config: T) -> Self
    where 
        T: FnOnce(CorsConfig) -> CorsConfig
    {
        self.set_cors(config(CorsConfig::default()))
    }

    /// Configures a web server with specified CORS configuration
    ///
    /// Default: `None`
    pub fn set_cors(mut self, mut config: CorsConfig) -> Self {
        match config.name.take() {
            Some(name) => self.cors.insert_named(name, config),
            None => self.cors.set_default(config),
        }
        self
    }
}

impl<'a> Route<'a> {
    /// Disables CORS for this route
    pub fn disable_cors(self) -> Self {
        self.cors_override(CorsOverride::Disabled)
    }

    /// Sets the default CORS policy for this route
    pub fn cors(self) -> Self {
        self.cors_override(CorsOverride::Inherit)
    }

    /// Sets the named CORS policy for this route
    pub fn cors_with(self, name: &str) -> Self {
        let policy = self.cors
            .get_named(name)
            .expect("cors policy")
            .clone();

        self.cors_override(CorsOverride::Named(policy))
    }
    
    #[inline]
    pub(crate) fn cors_override(self, cors: CorsOverride) -> Self {
        self.app
            .pipeline
            .endpoints_mut()
            .bind_cors(
                &self.method,
                self.pattern.as_ref(),
                cors
            );
        self
    }
}

impl<'a> RouteGroup<'a> {
    /// Disables CORS for this route
    pub fn disable_cors(&mut self) -> &mut Self {
        self.cors = CorsOverride::Disabled;
        self
    }

    /// Sets the default CORS policy for this route
    pub fn cors(&mut self) -> &mut Self {
        self.cors = CorsOverride::Disabled;
        self
    }

    /// Sets the named CORS policy for this route
    pub fn cors_with(&mut self, name: &str) -> &mut Self {
        let policy = self.app.cors
            .get_named(name)
            .expect("cors policy")
            .clone();

        self.cors = CorsOverride::Named(policy);
        self
    }
}

#[inline]
fn build_csv<I>(items: I) -> String
where
    I: IntoIterator,
    I::Item: AsRef<str>,
{
    let mut it = items.into_iter();
    let mut out = String::new();

    if let Some(first) = it.next() {
        out.push_str(first.as_ref());
        for item in it {
            out.push_str(", ");
            out.push_str(item.as_ref());
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use std::time::Duration;
    use hyper::{header::HeaderName, http::HeaderValue};
    use reqwest::Method;
    use crate::App;
    use super::*;
    
    #[test]
    fn it_creates_default_cors_config() {
        let config = CorsConfig::default();
        
        assert_eq!(config.allow_origins, None);
        assert_eq!(config.allow_headers, None);
        assert_eq!(config.allow_methods, None);
        assert_eq!(config.expose_headers, None);
        assert_eq!(config.max_age, Some(Duration::from_secs(DEFAULT_MAX_AGE)));
        assert!(!config.allow_credentials);
        assert!(config.include_vary);
    }
    
    #[test]
    fn it_creates_cors_config_with_allow_origin() {
        let config = CorsConfig::default()
            .with_origins(["https://example.com", "https://example.net"]);
        
        let allowed_origins = config.allow_origins.unwrap();
        
        assert!(allowed_origins.contains(&HeaderValue::from_static("https://example.com")));
        assert!(allowed_origins.contains(&HeaderValue::from_static("https://example.net")));
        assert!(!allowed_origins.contains(&HeaderValue::from_static("https://example.org")));
    }

    #[test]
    fn it_creates_cors_config_with_allow_any_origin() {
        let config = CorsConfig::default()
            .with_any_origin();

        assert_eq!(config.allow_origins, None);
    }

    #[test]
    fn it_creates_cors_config_with_allow_headers() {
        let config = CorsConfig::default()
            .with_headers(["Content-Type", "X-Correlation-Id"]);

        let allowed_headers = config.allow_headers.unwrap();

        assert!(allowed_headers.contains(&HeaderName::from_static("content-type")));
        assert!(allowed_headers.contains(&HeaderName::from_static("x-correlation-id")));
        assert!(!allowed_headers.contains(&HeaderName::from_static("x-some-header")));
    }

    #[test]
    fn it_creates_cors_config_with_allow_any_header() {
        let config = CorsConfig::default()
            .with_any_header();

        assert_eq!(config.allow_headers, None);
    }

    #[test]
    fn it_creates_cors_config_with_allow_methods() {
        let config = CorsConfig::default()
            .with_methods([Method::GET, Method::POST]);

        let allowed_methods = config.allow_methods.unwrap();

        assert!(allowed_methods.contains(&Method::GET));
        assert!(allowed_methods.contains(&Method::POST));
        assert!(!allowed_methods.contains(&Method::PUT));
    }

    #[test]
    fn it_creates_cors_config_with_allow_any_method() {
        let config = CorsConfig::default()
            .with_any_method();

        assert_eq!(config.allow_methods, None);
    }

    #[test]
    fn it_creates_cors_config_with_max_age() {
        let config = CorsConfig::default()
            .with_max_age(10);

        assert_eq!(config.max_age, Some(Duration::from_secs(10)));
    }
    
    #[test]
    fn it_creates_cors_config_without_max_age() {
        let config = CorsConfig::default()
            .without_max_age();

        assert_eq!(config.max_age, None);
    }

    #[test]
    fn it_creates_cors_config_without_vary_header() {
        let config = CorsConfig::default()
            .with_vary_header(false);

        assert!(!config.include_vary);
    }

    #[test]
    fn it_creates_cors_config_with_include_credentials() {
        let config = CorsConfig::default()
            .with_credentials(true);

        assert!(config.allow_credentials);
    }

    #[test]
    fn it_creates_cors_config_with_expose_headers() {
        let config = CorsConfig::default()
            .with_expose_headers(["Content-Type", "X-Correlation-Id"]);

        let exposed_headers = config.expose_headers.unwrap();

        assert!(exposed_headers.contains(&HeaderName::from_static("content-type")));
        assert!(exposed_headers.contains(&HeaderName::from_static("x-correlation-id")));
        assert!(!exposed_headers.contains(&HeaderName::from_static("x-some-header")));
    }
    
    #[test]
    fn it_configures_cors_for_app() {
        let app = App::new()
            .with_cors(|cors| cors
                .with_origins(["https://example.com"])
                .with_headers(["Content-Type"])
                .with_methods([Method::GET, Method::POST])
                .with_credentials(true)
                .with_vary_header(false));

        let config = app.cors.default.unwrap();

        if let Some(allowed_origins) = &config.allow_origins {
            assert!(allowed_origins.contains(&HeaderValue::from_static("https://example.com")));
            assert!(!allowed_origins.contains(&HeaderValue::from_static("https://example.org")));
        }

        let allow_methods = config.preflight.get(ACCESS_CONTROL_ALLOW_METHODS)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        assert!(allow_methods.contains("GET"));
        assert!(allow_methods.contains("POST"));
        assert!(!allow_methods.contains("PUT"));

        assert_eq!(config.preflight.get(ACCESS_CONTROL_ALLOW_HEADERS).unwrap(), "content-type");
        assert_eq!(config.preflight.get(ACCESS_CONTROL_MAX_AGE).unwrap(), "86400");

        assert!(config.normal.get(ACCESS_CONTROL_EXPOSE_HEADERS).is_none());

        assert_eq!(config.common.get(ACCESS_CONTROL_ALLOW_CREDENTIALS).unwrap(), "true");

        assert!(config.allow_credentials);
        assert!(config.vary_normal.is_none());
        assert!(config.vary_preflight.is_none());
    }

    #[test]
    fn it_sets_cors_for_app() {
        let config = CorsConfig::default()
            .with_origins(["https://example.com"])
            .with_headers(["Content-Type"])
            .with_methods([Method::GET, Method::POST])
            .with_credentials(true)
            .with_vary_header(false);
        
        let app = App::new()
            .set_cors(config);

        let config = app.cors.default.unwrap();

        if let Some(allowed_origins) = &config.allow_origins {
            assert!(allowed_origins.contains(&HeaderValue::from_static("https://example.com")));
            assert!(!allowed_origins.contains(&HeaderValue::from_static("https://example.org")));
        }

        let allow_methods = config.preflight.get(ACCESS_CONTROL_ALLOW_METHODS)
            .unwrap()
            .to_str()
            .unwrap()
            .to_string();

        assert!(allow_methods.contains("GET"));
        assert!(allow_methods.contains("POST"));
        assert!(!allow_methods.contains("PUT"));

        assert_eq!(config.preflight.get(ACCESS_CONTROL_ALLOW_HEADERS).unwrap(), "content-type");
        assert_eq!(config.preflight.get(ACCESS_CONTROL_MAX_AGE).unwrap(), "86400");

        assert!(config.normal.get(ACCESS_CONTROL_EXPOSE_HEADERS).is_none());

        assert_eq!(config.common.get(ACCESS_CONTROL_ALLOW_CREDENTIALS).unwrap(), "true");

        assert!(config.allow_credentials);
        assert!(config.vary_normal.is_none());
        assert!(config.vary_preflight.is_none());
    }
    
    #[test]
    fn it_returns_access_control_allow_origin_header() {
        let config = CorsConfig::default()
            .with_origins(["https://example.com"])
            .precompute();
        
        let origin = Some(HeaderValue::from_static("https://example.com"));
        let header = config.allow_origin(origin);
        
        assert!(header.is_some());
        
        assert_eq!(header.unwrap(), "https://example.com");
    }

    #[test]
    fn it_returns_access_control_allow_origin_header_with_wildcard() {
        let config = CorsConfig::default()
            .with_any_origin()
            .precompute();

        let origin = Some(HeaderValue::from_static("https://example.com"));
        let header = config.allow_origin(origin);

        assert!(header.is_some());

        assert_eq!(header.unwrap(), "*");
    }

    #[test]
    fn it_does_not_return_access_control_allow_origin_header_with_credentials() {
        let config = CorsConfig::default()
            .with_any_origin()
            .with_credentials(true)
            .precompute();

        let origin = Some(HeaderValue::from_static("https://example.com"));
        let header = config.allow_origin(origin);

        assert!(header.is_none());
    }

    #[test]
    fn it_does_not_return_access_control_allow_origin_header_for_empty_origin() {
        let config = CorsConfig::default()
            .with_origins(["https://example.com"])
            .precompute();

        let header = config.allow_origin(None);

        assert!(header.is_none());
    }

    #[test]
    fn it_does_not_return_access_control_allow_origin_header() {
        let config = CorsConfig::default()
            .with_origins(["https://example.net"])
            .precompute();

        let origin = Some(HeaderValue::from_static("https://example.com"));
        let header = config.allow_origin(origin);

        assert!(header.is_none());
    }

    #[test]
    fn it_returns_access_control_allow_headers_header() {
        let config = CorsConfig::default()
            .with_headers(["Content-Type"]);

        let header = config.allow_headers();

        assert!(header.is_some());

        assert_eq!(header.unwrap(), "content-type");
    }

    #[test]
    fn it_returns_access_control_allow_headers_header_with_wildcard() {
        let config = CorsConfig::default()
            .with_any_header();

        let header = config.allow_headers();

        assert!(header.is_some());

        assert_eq!(header.unwrap(), "*");
    }

    #[test]
    fn it_returns_access_control_allow_methods_header() {
        let config = CorsConfig::default()
            .with_methods([Method::GET]);

        let header = config.allow_methods();

        assert!(header.is_some());

        assert_eq!(header.unwrap(), "GET");
    }

    #[test]
    fn it_returns_access_control_allow_methods_header_with_wildcard() {
        let config = CorsConfig::default()
            .with_any_method();

        let header = config.allow_methods();

        assert!(header.is_some());

        assert_eq!(header.unwrap(), "*");
    }

    #[test]
    fn it_returns_access_control_expose_headers_header() {
        let config = CorsConfig::default()
            .with_expose_headers(["x-req-id"]);

        let header = config.expose_headers();

        assert!(header.is_some());

        assert_eq!(header.unwrap(), "x-req-id");
    }

    #[test]
    fn it_returns_access_control_expose_headers_header_with_wildcard() {
        let config = CorsConfig::default()
            .with_expose_any_header();

        let header = config.expose_headers();

        assert!(header.is_some());

        assert_eq!(header.unwrap(), "*");
    }

    #[test]
    fn it_returns_access_control_max_age_header() {
        let config = CorsConfig::default()
            .with_max_age(10);

        let header = config.max_age();

        assert!(header.is_some());

        assert_eq!(header.unwrap(), "10");
    }

    #[test]
    fn it_does_not_return_access_control_expose_headers_header_by_default() {
        let config = CorsConfig::default();

        let header = config.expose_headers();

        assert!(header.is_none());
    }

    #[test]
    fn it_does_not_return_access_control_max_age_header() {
        let config = CorsConfig::default()
            .without_max_age();

        let header = config.max_age();

        assert!(header.is_none());
    }

    #[test]
    fn it_returns_none_vary_preflight_header() {
        let config = CorsConfig::default();

        let header = config.vary_preflight();

        assert!(header.is_none());
    }

    #[test]
    fn it_returns_vary_preflight_header() {
        let config = CorsConfig::default()
            .with_origins(["http://www.example.com"]);

        let header = config.vary_preflight();

        assert_eq!(header.unwrap(), "origin, access-control-request-method, access-control-request-headers");
    }

    #[test]
    fn it_returns_none_vary_normal_header() {
        let config = CorsConfig::default();

        let header = config.vary_normal();

        assert!(header.is_none());
    }

    #[test]
    fn it_returns_vary_normal_header() {
        let config = CorsConfig::default()
            .with_origins(["http://www.example.com"]);

        let header = config.vary_normal();

        assert_eq!(header.unwrap(), "Origin");
    }

    #[test]
    fn it_does_not_return_vary_preflight_header() {
        let config = CorsConfig::default()
            .with_vary_header(false);

        let header = config.vary_preflight();

        assert!(header.is_none());
    }

    #[test]
    fn it_does_not_return_vary_normal_header() {
        let config = CorsConfig::default()
            .with_vary_header(false);

        let header = config.vary_normal();

        assert!(header.is_none());
    }

    #[test]
    fn it_doesnt_needs_vary_when_any_origin_and_allow_any_credentials_false() {
        let config = CorsConfig::default()
            .with_any_origin()
            .with_vary_header(true);

        assert!(!config.needs_vary());
    }

    #[test]
    fn it_needs_vary_with_origin() {
        let config = CorsConfig::default()
            .with_vary_header(true)
            .with_origins(["http://localhost:7878/"]);

        assert!(config.needs_vary());
    }

    #[test]
    fn it_does_not_return_access_control_allow_credentials_header() {
        let config = CorsConfig::default();

        let header = config.allow_credentials();

        assert!(header.is_none());
    }

    #[test]
    fn it_returns_access_control_allow_credentials_header() {
        let config = CorsConfig::default()
            .with_credentials(true);

        let header = config.allow_credentials();

        assert!(header.is_some());

        assert_eq!(header.unwrap(), "true");
    }
    
    #[test]
    fn it_validates_cors_config() {
        let config = CorsConfig::default();
        config.validate();
    }

    #[test]
    #[should_panic]
    fn it_panics_due_combining_any_origin_with_allow_credentials() {
        let config = CorsConfig::default()
            .with_any_origin()
            .with_credentials(true);
        config.validate();
    }

    #[test]
    #[should_panic]
    fn it_panics_due_combining_any_header_with_allow_credentials() {
        let config = CorsConfig::default()
            .with_origins(["http://localhost:7878/"])
            .with_any_header()
            .with_credentials(true);
        config.validate();
    }

    #[test]
    #[should_panic]
    fn it_panics_due_combining_any_method_with_allow_credentials() {
        let config = CorsConfig::default()
            .with_origins(["http://localhost:7878/"])
            .with_headers(["Content-Type"])
            .with_any_method()
            .with_credentials(true);
        config.validate();
    }

    #[test]
    #[should_panic]
    fn it_panics_due_combining_expose_any_headers_with_allow_credentials() {
        let config = CorsConfig::default()
            .with_origins(["http://localhost:7878/"])
            .with_headers(["Content-Type"])
            .with_methods([Method::GET, Method::POST])
            .with_expose_any_header()
            .with_credentials(true);
        config.validate();
    }
}