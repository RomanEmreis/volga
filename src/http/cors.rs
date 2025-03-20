//! CORS (Cross-Origin Resource Sharing) configuration

use crate::App;
use hyper::{
    http::{HeaderValue, HeaderName},
    header::{ORIGIN, ACCESS_CONTROL_REQUEST_METHOD, ACCESS_CONTROL_REQUEST_HEADERS},
    Method
};

use std::{
    collections::HashSet,
    time::Duration
};

const DEFAULT_MAX_AGE: u64 = 24 * 60 * 60; // 24 hours = 86,400 seconds
const WILDCARD_STR: &str = "*";
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
    allow_origins: Option<HashSet<&'static str>>,
    allow_headers: Option<HashSet<&'static str>>,
    allow_methods: Option<HashSet<Method>>,
    expose_headers: Option<HashSet<&'static str>>,
    max_age: Option<Duration>,
    allow_credentials: bool,
    vary_header: bool
}

impl Default for CorsConfig {
    #[inline]
    fn default() -> Self {
        Self {
            max_age: Some(Duration::from_secs(DEFAULT_MAX_AGE)),
            allow_credentials: false,
            vary_header: true,
            expose_headers: None,
            allow_origins: None,
            allow_headers: None,
            allow_methods: None,
        }
    }
}

impl CorsConfig {
    /// Configures CORS with allowed origins 
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
    pub fn with_origins<T>(mut self, origins: T) -> Self
    where
        T: IntoIterator<Item = &'static str>,
    {
        let allowed_origins = origins
            .into_iter()
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
    pub fn with_headers<T>(mut self, headers: T) -> Self
    where
        T: IntoIterator<Item = &'static str>,
    {
        let allowed_headers = headers
            .into_iter()
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
    pub fn with_methods<T>(mut self, methods: T) -> Self
    where
        T: IntoIterator<Item = Method>,
    {
        let allowed_methods = methods
            .into_iter()
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
        self.vary_header = include_vary;
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
    pub fn with_expose_headers<T>(mut self, headers: T) -> Self
    where
        T: IntoIterator<Item = &'static str>,
    {
        let allowed_headers = headers
            .into_iter()
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
        self.expose_headers = Some(HashSet::from([WILDCARD_STR]));
        self
    }

    /// Creates a value for the [`Access-Control-Allow-Origin`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Access-Control-Allow-Origin)
    /// HTTP header
    pub fn allow_origin(&self, origin: Option<&HeaderValue>) -> Option<HeaderValue> {
        match (&self.allow_origins, self.allow_credentials) {
            (Some(allow_origins), _) => origin
                .filter(|&o| allow_origins.contains(o.to_str().unwrap()))
                .cloned(),
            (None, false) => Some(WILDCARD_VALUE),
            (None, true) => None,
        }
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
                    .collect::<Vec<_>>().join(",");
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
                let allow_headers = allow_headers
                    .iter()
                    .copied()
                    .collect::<Vec<_>>().join(",");
                HeaderValue::from_str(&allow_headers).ok()
            }
        }
    }

    /// Creates a value for the [`Access-Control-Expose-Headers`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Access-Control-Expose-Headers)
    /// HTTP header
    pub fn expose_headers(&self) -> Option<HeaderValue> {
        match &self.expose_headers { 
            None => None,
            Some(expose_headers) => {
                let expose_headers = expose_headers
                    .iter()
                    .copied()
                    .collect::<Vec<_>>().join(",");
                HeaderValue::from_str(&expose_headers).ok()
            }
        }
    }

    /// Creates a value for the [`Access-Control-Max-Age`](https://developer.mozilla.org/en-US/docs/Web/HTTP/Reference/Headers/Access-Control-Max-Age)
    /// HTTP header
    pub fn max_age(&self) -> Option<HeaderValue> {
        match &self.max_age { 
            None => None,
            Some(max_age) => HeaderValue::from_str(&max_age.as_secs().to_string()).ok()
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

    /// Creates a value for the `Vary` HTTP header
    pub fn vary_header(&self) -> Option<HeaderValue> {
        if self.vary_header {
            let vary_header = DEFAULT_PREFLIGHT_HEADERS.join(",");
            HeaderValue::from_str(&vary_header).ok()
        } else {
            None
        }
    }
    
    /// Validates the [`CorsConfig`] and panics if it's invalid
    pub(crate) fn validate(&self) {
        if self.allow_credentials {
            assert!(
                self.allow_origins.is_some(),
                "CORS error: The `Access-Control-Allow-Credentials: true` cannot be used \
                with `Access-Control-Allow-Headers: *`"
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

            if let Some(expose_headers) = &self.expose_headers {
                assert!(
                    !expose_headers.contains(WILDCARD_STR),
                    "CORS error: The `Access-Control-Allow-Credentials: true` cannot be used \
                    with `Access-Control-Expose-Headers: *`"
                );   
            }
        }
    } 
}

impl App {
    /// Configures web server with specified CORS configuration
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
    pub fn with_cors<T>(mut self, config: T) -> Self
    where 
        T: FnOnce(CorsConfig) -> CorsConfig
    {
        self.cors_config = Some(config(self.cors_config.unwrap_or_default()));
        self
    }


    /// Configures web server with specified CORS configuration
    ///
    /// Default: `None`
    pub fn set_cors(mut self, config: CorsConfig) -> Self {
        self.cors_config = Some(config);
        self
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;
    use hyper::http::HeaderValue;
    use reqwest::Method;
    use crate::App;
    use super::{CorsConfig, DEFAULT_MAX_AGE};
    
    #[test]
    fn it_creates_default_cors_config() {
        let config = CorsConfig::default();
        
        assert_eq!(config.allow_origins, None);
        assert_eq!(config.allow_headers, None);
        assert_eq!(config.allow_methods, None);
        assert_eq!(config.expose_headers, None);
        assert_eq!(config.max_age, Some(Duration::from_secs(DEFAULT_MAX_AGE)));
        assert!(!config.allow_credentials);
        assert!(config.vary_header);
    }
    
    #[test]
    fn it_creates_cors_config_with_allow_origin() {
        let config = CorsConfig::default()
            .with_origins(["https://example.com", "https://example.net"]);
        
        let allowed_origins = config.allow_origins.unwrap();
        
        assert!(allowed_origins.contains(&"https://example.com"));
        assert!(allowed_origins.contains(&"https://example.net"));
        assert!(!allowed_origins.contains(&"https://example.org"));
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

        assert!(allowed_headers.contains(&"Content-Type"));
        assert!(allowed_headers.contains(&"X-Correlation-Id"));
        assert!(!allowed_headers.contains(&"X-Some-Header"));
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

        assert!(!config.vary_header);
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

        assert!(exposed_headers.contains(&"Content-Type"));
        assert!(exposed_headers.contains(&"X-Correlation-Id"));
        assert!(!exposed_headers.contains(&"X-Some-Header"));
    }

    #[test]
    fn it_creates_cors_config_with_expose_any_header() {
        let config = CorsConfig::default()
            .with_expose_any_header();

        assert!(config.expose_headers.unwrap().contains("*"));
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

        let config = app.cors_config.unwrap();
        let allowed_origins = config.allow_origins.unwrap();
        let allowed_headers = config.allow_headers.unwrap();
        let allowed_methods = config.allow_methods.unwrap();
        
        assert!(allowed_origins.contains(&"https://example.com"));
        assert!(!allowed_origins.contains(&"https://example.org"));

        assert!(allowed_headers.contains(&"Content-Type"));
        assert!(!allowed_headers.contains(&"X-Some-Header"));

        assert!(allowed_methods.contains(&Method::GET));
        assert!(allowed_methods.contains(&Method::POST));
        assert!(!allowed_methods.contains(&Method::PUT));

        assert_eq!(config.expose_headers, None);
        assert_eq!(config.max_age, Some(Duration::from_secs(DEFAULT_MAX_AGE)));
        assert!(config.allow_credentials);
        assert!(!config.vary_header);
    }

    #[test]
    fn it_sets_cors_for_app() {
        let config = CorsConfig::default()
            .with_origins(["https://example.com"])
            .with_headers(["Content-Type"])
            .with_methods([Method::GET, Method::POST]);
        
        let app = App::new()
            .set_cors(config)
            .with_cors(|cors| cors
                .without_max_age()
                .with_credentials(true)
                .with_vary_header(false));

        let config = app.cors_config.unwrap();
        let allowed_origins = config.allow_origins.unwrap();
        let allowed_headers = config.allow_headers.unwrap();
        let allowed_methods = config.allow_methods.unwrap();

        assert!(allowed_origins.contains(&"https://example.com"));
        assert!(!allowed_origins.contains(&"https://example.org"));

        assert!(allowed_headers.contains(&"Content-Type"));
        assert!(!allowed_headers.contains(&"X-Some-Header"));

        assert!(allowed_methods.contains(&Method::GET));
        assert!(allowed_methods.contains(&Method::POST));
        assert!(!allowed_methods.contains(&Method::PUT));

        assert_eq!(config.expose_headers, None);
        assert_eq!(config.max_age, None);
        assert!(config.allow_credentials);
        assert!(!config.vary_header);
    }
    
    #[test]
    fn it_returns_access_control_allow_origin_header() {
        let config = CorsConfig::default()
            .with_origins(["https://example.com"]);
        
        let origin = Some(HeaderValue::from_static("https://example.com"));
        let header = config.allow_origin(origin.as_ref());
        
        assert!(header.is_some());
        
        assert_eq!(header.unwrap(), "https://example.com");
    }

    #[test]
    fn it_returns_access_control_allow_origin_header_with_wildcard() {
        let config = CorsConfig::default()
            .with_any_origin();

        let origin = Some(HeaderValue::from_static("https://example.com"));
        let header = config.allow_origin(origin.as_ref());

        assert!(header.is_some());

        assert_eq!(header.unwrap(), "*");
    }

    #[test]
    fn it_does_not_return_access_control_allow_origin_header_with_credentials() {
        let config = CorsConfig::default()
            .with_any_origin()
            .with_credentials(true);

        let origin = Some(HeaderValue::from_static("https://example.com"));
        let header = config.allow_origin(origin.as_ref());

        assert!(header.is_none());
    }

    #[test]
    fn it_does_not_return_access_control_allow_origin_header_for_empty_origin() {
        let config = CorsConfig::default()
            .with_origins(["https://example.com"]);

        let header = config.allow_origin(None);

        assert!(header.is_none());
    }

    #[test]
    fn it_does_not_return_access_control_allow_origin_header() {
        let config = CorsConfig::default()
            .with_origins(["https://example.net"]);

        let origin = Some(HeaderValue::from_static("https://example.com"));
        let header = config.allow_origin(origin.as_ref());

        assert!(header.is_none());
    }

    #[test]
    fn it_returns_access_control_allow_headers_header() {
        let config = CorsConfig::default()
            .with_headers(["Content-Type"]);

        let header = config.allow_headers();

        assert!(header.is_some());

        assert_eq!(header.unwrap(), "Content-Type");
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
    fn it_returns_vary_header() {
        let config = CorsConfig::default();

        let header = config.vary_header();

        assert!(header.is_some());

        assert_eq!(header.unwrap(), "origin,access-control-request-method,access-control-request-headers");
    }

    #[test]
    fn it_does_not_return_vary_header() {
        let config = CorsConfig::default()
            .with_vary_header(false);

        let header = config.vary_header();

        assert!(header.is_none());
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