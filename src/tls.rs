use futures_util::TryFutureExt;
use hyper_util::{rt::TokioIo, server::graceful::GracefulShutdown};
use crate::{
    App, 
    app::AppInstance, 
    error::{Error, handler::call_weak_err_handler}
};

use std::{
    fmt, 
    net::SocketAddr, 
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use tokio::{
    net::{TcpListener, TcpStream},
    sync::watch,
    time::sleep
};

use tokio_rustls::{
    rustls::{
        pki_types::{
            pem::PemObject,
            CertificateDer,
            PrivateKeyDer
        },
        server::WebPkiClientVerifier,
        RootCertStore,
        ServerConfig,
    },
    TlsAcceptor
};

use crate::tls::https_redirect::HttpsRedirectionMiddleware;

#[cfg(any(
    all(feature = "http1", feature = "http2"),
    all(feature = "http2", not(feature = "http1"))
))]
use hyper::server::conn::http2;

#[cfg(any(
    all(feature = "http1", feature = "http2"),
    all(feature = "http2", not(feature = "http1"))
))]
use hyper_util::rt::TokioExecutor;

#[cfg(all(feature = "http1", not(feature = "http2")))]
use hyper::server::conn::http1;

pub(super) mod https_redirect;

const CERT_FILE_NAME: &str = "cert.pem";
const KEY_FILE_NAME: &str = "key.pem";
const DEFAULT_PORT: u16 = 7879;
const DEFAULT_MAX_AGE: u64 = 30 * 24 * 60 * 60; // 30 days = 2,592,000 seconds

/// Represents a TLS (Transport Layer Security) configuration options
pub struct TlsConfig {
    /// Path to a certificate
    pub cert: PathBuf,
    
    /// Path to a private key
    pub key: PathBuf,
    
    /// HTTPS redirection configuration options
    pub https_redirection_config: RedirectionConfig,
    
    /// HSTS configuration options
    hsts_config: HstsConfig,
    
    /// Client Auth options
    client_auth: ClientAuth,
}

/// Represents an HTTPS redirection configuration options
#[derive(Clone)]
pub struct RedirectionConfig {
    /// Specifies whether HTTPS redirection is enabled
    /// 
    /// Default: `false`
    pub enabled: bool,
    
    /// Specifies HTTP port for redirection middleware
    /// 
    /// Default: `7879`
    pub http_port: u16,
} 

/// Represents a HSTS (HTTP Strict Transport Security Protocol) configuration options
pub struct HstsConfig {
    /// Specifies whether include a `preload` to HSTS header
    /// 
    /// Default: `true`
    preload: bool,
    
    /// Specifies whether include a `includeSubDomains` to HSTS header
    /// 
    /// Default: `true`
    include_sub_domains: bool,
    
    /// Max age for HSTS header
    /// 
    /// Default: 30 days (2,592,000 seconds)
    max_age: Duration,
    
    /// A list of hosts names that will not add the HSTS header.
    exclude_hosts: Vec<&'static str>
}

/// Represents a types of Client Auth
#[derive(Debug, PartialEq)]
enum ClientAuth {
    None,
    Optional(PathBuf),
    Required(PathBuf)
}

impl Default for RedirectionConfig {
    fn default() -> Self {
        Self { 
            enabled: false,
            http_port: DEFAULT_PORT,
        }
    }
}

impl Default for HstsConfig {
    fn default() -> Self {
        Self {
            preload: true,
            include_sub_domains: true,
            max_age: Duration::from_secs(DEFAULT_MAX_AGE), // 30 days = 2,592,000 seconds
            exclude_hosts: Vec::new()
        }
    }
}

impl Default for TlsConfig {
    fn default() -> Self {
        let path = std::env::current_dir().unwrap_or_default();
        let cert = path.join(CERT_FILE_NAME);
        let key = path.join(KEY_FILE_NAME);
        Self { 
            https_redirection_config: RedirectionConfig::default(),
            client_auth: ClientAuth::None,
            hsts_config: HstsConfig::default(),
            key, 
            cert, 
        }
    }
}

impl fmt::Display for HstsConfig {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut str = String::new();
        str.push_str(&format!("max-age={}", self.max_age.as_secs()));
        
        if self.include_sub_domains {
            str.push_str("; includeSubDomains");
        }

        if self.preload {
            str.push_str("; preload");
        }
        
        f.write_str(&str)
    }
}

impl TlsConfig {
    /// Creates a new, default TLS configuration
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Creates a configuration by loading cert and key files with default names from specified folder
    pub fn from_pem(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        let cert = path.join(CERT_FILE_NAME);
        let key = path.join(KEY_FILE_NAME);
        Self {
            https_redirection_config: RedirectionConfig::default(),
            client_auth: ClientAuth::None,
            hsts_config: HstsConfig::default(),
            key, 
            cert, 
        }
    }

    /// Creates a configuration by specifying path to cert and key files specifically
    pub fn from_pem_files(cert_file_path: &str, key_file_path: &str) -> Self {
        Self { 
            key: key_file_path.into(), 
            cert: cert_file_path.into(),
            client_auth: ClientAuth::None,
            https_redirection_config: RedirectionConfig::default(),
            hsts_config: HstsConfig::default(),
        }
    }
    
    /// Configure the path to the certificate
    pub fn with_cert_path(mut self, path: impl AsRef<Path>) -> Self {
        self.cert = path.as_ref().into();
        self
    }

    /// Configure the path to the private key
    pub fn with_key_path(mut self, path: impl AsRef<Path>) -> Self {
        self.key = path.as_ref().into();
        self
    }
    
    /// Configures the trust anchor for optional TLS client authentication.
    /// 
    /// Default: `None`
    pub fn with_optional_client_auth(mut self, path: impl AsRef<Path>) -> Self {
        self.client_auth = ClientAuth::Optional(path.as_ref().into());
        self
    }

    /// Configures the trust anchor for required TLS client authentication.
    ///
    /// Default: `None`
    pub fn with_required_client_auth(mut self, path: impl AsRef<Path>) -> Self {
        self.client_auth = ClientAuth::Required(path.as_ref().into());
        self
    }

    /// Configures web server to redirect HTTP requests to HTTPS
    /// 
    /// Default: `false`
    pub fn with_https_redirection(mut self) -> Self {
        self.https_redirection_config.enabled = true;
        self
    }

    /// Configures the port for HTTP listener that redirects to HTTPS
    pub fn with_http_port(mut self, port: u16) -> Self {
        self.https_redirection_config.http_port = port;
        self
    }

    /// Configures HSTS header
    ///
    /// Default values:
    /// - preload: `true`
    /// - include_sub_domains: `true`
    /// - max-age: 30 days (2,592,000 seconds)
    /// - exclude_hosts: empty list
    pub fn with_hsts(mut self, hsts_config: HstsConfig) -> Self {
        self.hsts_config = hsts_config;
        self
    }
    
    /// Configures whether to set `preload` in HSTS header
    /// 
    /// Default value: `true`
    pub fn with_hsts_preload(mut self, preload: bool) -> Self {
        self.hsts_config.preload = preload;
        self
    }

    /// Configures whether to set `includeSubDomains` in HSTS header
    /// 
    /// Default: `true`
    pub fn with_hsts_sub_domains(mut self, include: bool) -> Self {
        self.hsts_config.include_sub_domains = include;
        self
    }

    /// Configures `max_age` for HSTS header
    /// 
    /// Default: 30 days (2,592,000 seconds)
    pub fn with_hsts_max_age(mut self, max_age: Duration) -> Self {
        self.hsts_config.max_age = max_age;
        self
    }

    /// Configures a list of host names that will not add the HSTS header.
    /// 
    /// Default: empty list
    pub fn with_hsts_exclude_hosts(mut self, exclude_hosts: &[&'static str]) -> Self {
        self.hsts_config.exclude_hosts = exclude_hosts.into();
        self
    }

    pub(super) fn build(self) -> Result<ServerConfig, Error> {
        let certs = Self::load_cert_file(&self.cert)?;
        let key = Self::load_key_file(&self.key)?;
        
        let builder = match self.client_auth { 
            ClientAuth::None => ServerConfig::builder().with_no_client_auth(),
            ClientAuth::Optional(trust_anchor) => {
                let verifier =
                    WebPkiClientVerifier::builder(Self::read_trust_anchor(trust_anchor)?.into())
                        .allow_unauthenticated()
                        .build()
                        .map_err(Error::from)?;
                ServerConfig::builder().with_client_cert_verifier(verifier)
            },
            ClientAuth::Required(trust_anchor) => {
                let verifier =
                    WebPkiClientVerifier::builder(Self::read_trust_anchor(trust_anchor)?.into())
                        .build()
                        .map_err(Error::from)?;
                ServerConfig::builder().with_client_cert_verifier(verifier)
            }
        };
        
        let mut config = builder
            .with_single_cert(certs, key)
            .map_err(Error::from)?;
        
        config.alpn_protocols = vec![
            #[cfg(feature = "http2")]
            b"h2".into(),
            b"http/1.1".into(),
            b"http/1.0".into()
        ];
        
        Ok(config)
    }
    
    #[inline]
    fn load_cert_file<'a>(path: impl AsRef<Path>) -> Result<Vec<CertificateDer<'a>>, Error> {
        CertificateDer::pem_file_iter(path)
            .map_err(Error::from)?
            .collect::<Result<Vec<_>, _>>()
            .map_err(Error::from)
    }
    
    #[inline]
    fn load_key_file<'a>(path: impl AsRef<Path>) -> Result<PrivateKeyDer<'a>, Error> {
        PrivateKeyDer::from_pem_file(path).map_err(Error::from)
    }

    fn read_trust_anchor(path: impl AsRef<Path>) -> Result<RootCertStore, Error> {
        let trust_anchors = Self::load_cert_file(path)?;
        let mut store = RootCertStore::empty();
        let (added, _skipped) = store.add_parsable_certificates(trust_anchors);
        if added == 0 {
            return Err(Error::server_error("TLS config error: certificate parse error"));
        }
        Ok(store)
    }
}

impl From<tokio_rustls::rustls::Error> for Error {
    #[inline]
    fn from(err: tokio_rustls::rustls::Error) -> Self {
        Self::server_error(format!("TLS config error: {}", err))
    }
}

impl From<tokio_rustls::rustls::pki_types::pem::Error> for Error {
    fn from(err: tokio_rustls::rustls::pki_types::pem::Error) -> Self {
        Self::server_error(format!("TLS config error: {}", err))
    }
}

impl From<tokio_rustls::rustls::server::VerifierBuilderError> for Error {
    #[inline]
    fn from(err: tokio_rustls::rustls::server::VerifierBuilderError) -> Self {
        Self::server_error(format!("TLS config error: {}", err))
    }
}

/// TLS specific impl for [`AppInstance`]
impl AppInstance {
    #[inline]
    pub(super) fn acceptor(&self) -> Option<TlsAcceptor> {
        self.acceptor.clone()
    }
}

/// TLS specific impl for [`App`]
impl App {
    /// Configures web server with specified TLS configuration
    /// 
    /// Default: `None`
    pub fn with_tls(mut self, config: TlsConfig) -> Self {
        self.tls_config = Some(config);
        self
    }

    /// Configures web server with TLS configuration 
    /// loaded from cert and key files with default names from specified folder 
    ///
    /// Default: `None`
    pub fn with_tls_from_pem(self, path: impl AsRef<Path>) -> Self {
        self.with_tls(TlsConfig::from_pem(path))
    }

    /// Configures web server with TLS configuration
    /// loaded from provided cert and key files specifically
    /// 
    /// Default: `None`
    pub fn with_tls_from_pem_files(self, cert_file_path: &str, key_file_path: &str) -> Self {
        self.with_tls(TlsConfig::from_pem_files(cert_file_path, key_file_path))
    }

    /// If the [`TlsConfig`] has been specified, 
    /// it configures the trust anchor for optional TLS client authentication.
    ///
    /// Default: `None`
    ///
    /// # Example
    /// ```no_run
    /// use volga::App;
    /// use volga::tls::{TlsConfig, HstsConfig};
    ///
    /// let app = App::new()
    ///     .with_tls(TlsConfig::new())
    ///     .with_optional_client_auth("path/to/pem");
    /// ```
    pub fn with_optional_client_auth(mut self, path: impl AsRef<Path>) -> Self {
        self.tls_config = self
            .tls_config
            .map(|config| config.with_optional_client_auth(path));
        self
    }

    /// If the [`TlsConfig`] has been specified, 
    /// it configures the trust anchor for required TLS client authentication.
    ///
    /// Default: `None`
    ///
    /// # Example
    /// ```no_run
    /// use volga::App;
    /// use volga::tls::{TlsConfig, HstsConfig};
    ///
    /// let app = App::new()
    ///     .with_tls(TlsConfig::new())
    ///     .with_required_client_auth("path/to/pem");
    /// ```
    pub fn with_required_client_auth(mut self, path: impl AsRef<Path>) -> Self {
        self.tls_config = self
            .tls_config
            .map(|config| config.with_required_client_auth(path));
        self
    }

    /// If the [`TlsConfig`] has been specified,
    /// it configures web server to redirect HTTP requests to HTTPS
    ///
    /// Default: `false`
    ///
    /// # Example
    /// ```no_run
    /// use volga::App;
    /// use volga::tls::{TlsConfig, HstsConfig};
    ///
    /// let app = App::new()
    ///     .with_tls(TlsConfig::new())
    ///     .with_https_redirection();
    /// ```
    pub fn with_https_redirection(mut self) -> Self {
        self.tls_config = self
            .tls_config
            .map(|config| config.with_https_redirection());
        self
    }

    /// If the [`TlsConfig`] has been specified, 
    /// it configures the port for HTTP listener that redirects to HTTPS
    ///
    /// # Example
    /// ```no_run
    /// use volga::App;
    /// use volga::tls::{TlsConfig, HstsConfig};
    ///
    /// let app = App::new()
    ///     .with_tls(TlsConfig::new())
    ///     .with_http_port(5000);
    /// ```
    pub fn with_http_port(mut self, port: u16) -> Self {
        self.tls_config = self
            .tls_config
            .map(|config| config.with_http_port(port));
        self
    }

    /// If the [`TlsConfig`] has been specified, it configures HSTS header
    ///
    /// Default values:
    /// - preload: `true`
    /// - include_sub_domains: `true`
    /// - max-age: 30 days (2,592,000 seconds)
    /// - exclude_hosts: empty list
    /// 
    /// # Example
    /// ```no_run
    /// use volga::App;
    /// use volga::tls::{TlsConfig, HstsConfig};
    /// 
    /// let app = App::new()
    ///     .with_tls(TlsConfig::new())
    ///     .with_hsts(HstsConfig::default());
    /// ```
    pub fn with_hsts(mut self, hsts_config: HstsConfig) -> Self {
        self.tls_config = self
            .tls_config
            .map(|config| config.with_hsts(hsts_config));
        self
    }

    /// If the [`TlsConfig`] has been specified, 
    /// it configures whether to set `preload` in HSTS header
    ///
    /// Default value: `true`
    /// 
    /// # Example
    /// ```no_run
    /// use volga::App;
    /// use volga::tls::{TlsConfig, HstsConfig};
    ///
    /// let app = App::new()
    ///     .with_tls(TlsConfig::new())
    ///     .with_hsts_preload(true);
    /// ```
    pub fn with_hsts_preload(mut self, preload: bool) -> Self {
        self.tls_config = self
            .tls_config
            .map(|config| config.with_hsts_preload(preload));
        self
    }

    /// If the [`TlsConfig`] has been specified, 
    /// it configures whether to set `includeSubDomains` in HSTS header
    ///
    /// Default: `true`
    ///
    /// # Example
    /// ```no_run
    /// use volga::App;
    /// use volga::tls::{TlsConfig, HstsConfig};
    ///
    /// let app = App::new()
    ///     .with_tls(TlsConfig::new())
    ///     .with_hsts_sub_domains(true);
    /// ```
    pub fn with_hsts_sub_domains(mut self, include: bool) -> Self {
        self.tls_config = self
            .tls_config
            .map(|config| config.with_hsts_sub_domains(include));
        self
    }

    /// Configures `max_age` for HSTS header
    ///
    /// Default: 30 days (2,592,000 seconds)
    ///
    /// # Example
    /// ```no_run
    /// use volga::App;
    /// use volga::tls::{TlsConfig, HstsConfig};
    /// use std::time::Duration;
    ///
    /// let app = App::new()
    ///     .with_tls(TlsConfig::new())
    ///     .with_hsts_max_age(Duration::from_secs(60));
    /// ```
    pub fn with_hsts_max_age(mut self, max_age: Duration) -> Self {
        self.tls_config = self
            .tls_config
            .map(|config| config.with_hsts_max_age(max_age));
        self
    }

    /// Configures a list of host names that will not add the HSTS header.
    ///
    /// Default: empty list
    ///
    /// # Example
    /// ```no_run
    /// use volga::App;
    /// use volga::tls::{TlsConfig, HstsConfig};
    ///
    /// let exclude = ["www.example.com", "www.example.net"];
    /// 
    /// let app = App::new()
    ///     .with_tls(TlsConfig::new())
    ///     .with_hsts_exclude_hosts(&exclude);
    /// ```
    pub fn with_hsts_exclude_hosts(mut self, exclude_hosts: &[&'static str]) -> Self {
        self.tls_config = self
            .tls_config
            .map(|config| config.with_hsts_exclude_hosts(exclude_hosts));
        self
    }
    
    /// Adds middleware for using HSTS, which adds the `Strict-Transport-Security` HTTP header.
    pub fn use_hsts(&mut self) -> &mut Self {
        if let Some(tls_config) = &self.tls_config {
            use crate::headers::{Header, Host, STRICT_TRANSPORT_SECURITY};
            
            let hsts_header_value = tls_config.hsts_config.to_string();
            let exclude_hosts = tls_config.hsts_config.exclude_hosts.clone();
            
            let is_excluded = move |host: Option<&str>| {
                if exclude_hosts.is_empty() { 
                    return false;
                }
                if let Some(host) = host {
                    return exclude_hosts.contains(&host);
                }
                false
            };
            
            self.use_middleware(move |ctx, next| {
                let hsts_header = STRICT_TRANSPORT_SECURITY.clone();
                let hsts_header_value = hsts_header_value.clone();
                let is_excluded = is_excluded.clone();
                
                async move {
                    let host = ctx.extract::<Header<Host>>()?;
                    let error_handler = ctx.error_handler();
                    let uri = ctx.request.uri().clone();
                    let http_result = next(ctx)
                        .or_else(|err| async { call_weak_err_handler(error_handler, &uri, err).await })
                        .await;

                    if !is_excluded(host.to_str().ok()) {
                        http_result.map(|mut response| {
                            response
                                .headers_mut()
                                .append(hsts_header, hsts_header_value.parse().unwrap());
                            response
                        })
                    } else { 
                        http_result
                    }
                }
            });
        }
        self
    }
    
    pub(super) fn run_https_redirection_middleware(
        socket: SocketAddr, 
        http_port: u16,
        shutdown_tx: Arc<watch::Sender<()>>
    ) {
        tokio::spawn(async move {
            let https_port = socket.port();
            let socket = SocketAddr::new(socket.ip(), http_port);
            #[cfg(feature = "tracing")]
            tracing::info!("listening on: http://{socket}");
            
            if let Ok(tcp_listener) = TcpListener::bind(socket).await {
                let graceful_shutdown = GracefulShutdown::new();
                loop {
                    let (stream, _) = tokio::select! {
                        _ = shutdown_tx.closed() => break,
                        Ok(connection) = tcp_listener.accept() => connection
                    };
                    Self::serve_http_redirection(https_port, stream, &graceful_shutdown);
                }
                tokio::select! {
                    _ = sleep(Duration::from_secs(super::app::GRACEFUL_SHUTDOWN_TIMEOUT)) => (),
                    _ = graceful_shutdown.shutdown() => {
                        #[cfg(feature = "tracing")]
                        tracing::info!("shutting down HTTPS redirection...");
                    },
                }
            } else {
                #[cfg(feature = "tracing")]
                tracing::error!("unable to start HTTPS redirection listener");
            }
        });
    }
    
    #[inline]
    fn serve_http_redirection(
        https_port: u16, 
        stream: TcpStream, 
        graceful_shutdown: &GracefulShutdown
    ) {
        let io = TokioIo::new(stream);

        #[cfg(all(feature = "http1", not(feature = "http2")))]
        let connection_builder = http1::Builder::new();

        #[cfg(any(
            all(feature = "http1", feature = "http2"),
            all(feature = "http2", not(feature = "http1"))
        ))]
        let connection_builder = http2::Builder::new(TokioExecutor::new());

        let connection = connection_builder.serve_connection(
            io,
            HttpsRedirectionMiddleware::new(https_port));
        
        let connection = graceful_shutdown.watch(connection);
        tokio::spawn(async move {
            if let Err(_err) = connection.await {
                #[cfg(feature = "tracing")]
                tracing::error!("error serving connection: {_err:#}");
            }
        });
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::time::Duration;
    use super::{
        TlsConfig, 
        HstsConfig, 
        RedirectionConfig,
        ClientAuth,
        KEY_FILE_NAME,
        CERT_FILE_NAME,
        DEFAULT_PORT,
        DEFAULT_MAX_AGE
    };
    
    #[test]
    fn it_creates_new_tls_config() {
        let tls_config = TlsConfig::new();
        
        let path = std::env::current_dir().unwrap_or_default();

        assert_eq!(tls_config.key, path.join(KEY_FILE_NAME));
        assert_eq!(tls_config.cert, path.join(CERT_FILE_NAME));
        assert_eq!(tls_config.client_auth, ClientAuth::None);
        
        assert_eq!(tls_config.hsts_config.exclude_hosts.len(), 0);
        assert_eq!(tls_config.hsts_config.max_age, Duration::from_secs(DEFAULT_MAX_AGE));
        assert!(tls_config.hsts_config.preload);
        assert!(tls_config.hsts_config.include_sub_domains);
        
        assert!(!tls_config.https_redirection_config.enabled);
        assert_eq!(tls_config.https_redirection_config.http_port, DEFAULT_PORT);
    }

    #[test]
    fn it_creates_default_tls_config() {
        let tls_config = TlsConfig::default();

        let path = std::env::current_dir().unwrap_or_default();
        
        assert_eq!(tls_config.key, path.join(KEY_FILE_NAME));
        assert_eq!(tls_config.cert, path.join(CERT_FILE_NAME));
        assert_eq!(tls_config.client_auth, ClientAuth::None);

        assert_eq!(tls_config.hsts_config.exclude_hosts.len(), 0);
        assert_eq!(tls_config.hsts_config.max_age, Duration::from_secs(DEFAULT_MAX_AGE));
        assert!(tls_config.hsts_config.preload);
        assert!(tls_config.hsts_config.include_sub_domains);

        assert!(!tls_config.https_redirection_config.enabled);
        assert_eq!(tls_config.https_redirection_config.http_port, DEFAULT_PORT);
    }

    #[test]
    fn it_creates_tls_config_from_pem() {
        let tls_config = TlsConfig::from_pem("tls");

        let path = PathBuf::from("tls");

        assert_eq!(tls_config.key, path.join(KEY_FILE_NAME));
        assert_eq!(tls_config.cert, path.join(CERT_FILE_NAME));
        assert_eq!(tls_config.client_auth, ClientAuth::None);

        assert_eq!(tls_config.hsts_config.exclude_hosts.len(), 0);
        assert_eq!(tls_config.hsts_config.max_age, Duration::from_secs(DEFAULT_MAX_AGE));
        assert!(tls_config.hsts_config.preload);
        assert!(tls_config.hsts_config.include_sub_domains);

        assert!(!tls_config.https_redirection_config.enabled);
        assert_eq!(tls_config.https_redirection_config.http_port, DEFAULT_PORT);
    }

    #[test]
    fn it_creates_tls_config_with_hsts() {
        let tls_config = TlsConfig::from_pem("tls")
            .with_hsts(HstsConfig { 
                max_age: Duration::from_secs(1),
                preload: false, 
                include_sub_domains: false, 
                exclude_hosts: vec!["example.com"]
            });

        let path = PathBuf::from("tls");

        assert_eq!(tls_config.key, path.join(KEY_FILE_NAME));
        assert_eq!(tls_config.cert, path.join(CERT_FILE_NAME));
        assert_eq!(tls_config.client_auth, ClientAuth::None);

        assert_eq!(tls_config.hsts_config.exclude_hosts.len(), 1);
        assert_eq!(tls_config.hsts_config.max_age, Duration::from_secs(1));
        assert!(!tls_config.hsts_config.preload);
        assert!(!tls_config.hsts_config.include_sub_domains);

        assert!(!tls_config.https_redirection_config.enabled);
        assert_eq!(tls_config.https_redirection_config.http_port, DEFAULT_PORT);
    }

    #[test]
    fn it_creates_tls_config_with_hsts_preload() {
        let tls_config = TlsConfig::from_pem("tls")
            .with_hsts_preload(false);

        let path = PathBuf::from("tls");

        assert_eq!(tls_config.key, path.join(KEY_FILE_NAME));
        assert_eq!(tls_config.cert, path.join(CERT_FILE_NAME));
        assert_eq!(tls_config.client_auth, ClientAuth::None);

        assert_eq!(tls_config.hsts_config.exclude_hosts.len(), 0);
        assert_eq!(tls_config.hsts_config.max_age, Duration::from_secs(DEFAULT_MAX_AGE));
        assert!(!tls_config.hsts_config.preload);
        assert!(tls_config.hsts_config.include_sub_domains);

        assert!(!tls_config.https_redirection_config.enabled);
        assert_eq!(tls_config.https_redirection_config.http_port, DEFAULT_PORT);
    }

    #[test]
    fn it_creates_tls_config_with_hsts_sub_domains() {
        let tls_config = TlsConfig::from_pem("tls")
            .with_hsts_sub_domains(false);

        let path = PathBuf::from("tls");

        assert_eq!(tls_config.key, path.join(KEY_FILE_NAME));
        assert_eq!(tls_config.cert, path.join(CERT_FILE_NAME));
        assert_eq!(tls_config.client_auth, ClientAuth::None);

        assert_eq!(tls_config.hsts_config.exclude_hosts.len(), 0);
        assert_eq!(tls_config.hsts_config.max_age, Duration::from_secs(DEFAULT_MAX_AGE));
        assert!(tls_config.hsts_config.preload);
        assert!(!tls_config.hsts_config.include_sub_domains);

        assert!(!tls_config.https_redirection_config.enabled);
        assert_eq!(tls_config.https_redirection_config.http_port, DEFAULT_PORT);
    }

    #[test]
    fn it_creates_tls_config_with_hsts_max_age() {
        let tls_config = TlsConfig::from_pem("tls")
            .with_hsts_max_age(Duration::from_secs(5));

        let path = PathBuf::from("tls");

        assert_eq!(tls_config.key, path.join(KEY_FILE_NAME));
        assert_eq!(tls_config.cert, path.join(CERT_FILE_NAME));
        assert_eq!(tls_config.client_auth, ClientAuth::None);

        assert_eq!(tls_config.hsts_config.exclude_hosts.len(), 0);
        assert_eq!(tls_config.hsts_config.max_age, Duration::from_secs(5));
        assert!(tls_config.hsts_config.preload);
        assert!(tls_config.hsts_config.include_sub_domains);

        assert!(!tls_config.https_redirection_config.enabled);
        assert_eq!(tls_config.https_redirection_config.http_port, DEFAULT_PORT);
    }

    #[test]
    fn it_creates_tls_config_with_hsts_exclude_hosts() {
        let tls_config = TlsConfig::from_pem("tls")
            .with_hsts_exclude_hosts(&["example.com"]);

        let path = PathBuf::from("tls");

        assert_eq!(tls_config.key, path.join(KEY_FILE_NAME));
        assert_eq!(tls_config.cert, path.join(CERT_FILE_NAME));
        assert_eq!(tls_config.client_auth, ClientAuth::None);

        assert_eq!(tls_config.hsts_config.exclude_hosts.len(), 1);
        assert_eq!(tls_config.hsts_config.max_age, Duration::from_secs(DEFAULT_MAX_AGE));
        assert!(tls_config.hsts_config.preload);
        assert!(tls_config.hsts_config.include_sub_domains);

        assert!(!tls_config.https_redirection_config.enabled);
        assert_eq!(tls_config.https_redirection_config.http_port, DEFAULT_PORT);
    }

    #[test]
    fn it_creates_default_hsts_config() {
        let hsts_config = HstsConfig::default();

        assert_eq!(hsts_config.exclude_hosts.len(), 0);
        assert_eq!(hsts_config.max_age, Duration::from_secs(DEFAULT_MAX_AGE));
        assert!(hsts_config.preload);
        assert!(hsts_config.include_sub_domains);
    }

    #[test]
    fn it_creates_default_redirect_config() {
        let https_redirection_config = RedirectionConfig::default();

        assert!(!https_redirection_config.enabled);
        assert_eq!(https_redirection_config.http_port, DEFAULT_PORT);
    }
    
    #[test]
    fn it_displays_hsts_config() {
        let hsts_config = HstsConfig::default();
        
        let hsts_string = hsts_config.to_string();
        
        assert_eq!(hsts_string, "max-age=2592000; includeSubDomains; preload");
    }
}