﻿use crate::app::{App, AppInstance};

use std::{
    fmt, 
    io::{Result, Error, ErrorKind}, 
    net::SocketAddr, 
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};

use hyper_util::{rt::TokioIo, server::graceful::GracefulShutdown};

use tokio::net::{TcpListener, TcpStream};
use tokio::sync::watch;
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
    pub cert: PathBuf,
    pub key: PathBuf,
    pub https_redirection_config: RedirectionConfig,
    hsts_config: HstsConfig,
    client_auth: ClientAuth,
}

/// Represents an HTTPS redirection configuration options
#[derive(Clone)]
pub struct RedirectionConfig {
    pub enabled: bool,
    pub http_port: u16,
} 

/// Represents a HSTS (HTTP Strict Transport Security Protocol) configuration options
pub struct HstsConfig {
    preload: bool,
    include_sub_domains: bool,
    max_age: Duration,
    exclude_hosts: Vec<&'static str>
}

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

    pub(super) fn build(self) -> Result<ServerConfig> {
        let certs = Self::load_cert_file(&self.cert)?;
        let key = Self::load_key_file(&self.key)?;
        
        let builder = match self.client_auth { 
            ClientAuth::None => ServerConfig::builder().with_no_client_auth(),
            ClientAuth::Optional(trust_anchor) => {
                let verifier =
                    WebPkiClientVerifier::builder(Self::read_trust_anchor(trust_anchor)?.into())
                        .allow_unauthenticated()
                        .build()
                        .map_err(TlsError::from_rustls_auth_error)?;
                ServerConfig::builder().with_client_cert_verifier(verifier)
            },
            ClientAuth::Required(trust_anchor) => {
                let verifier =
                    WebPkiClientVerifier::builder(Self::read_trust_anchor(trust_anchor)?.into())
                        .build()
                        .map_err(TlsError::from_rustls_auth_error)?;
                ServerConfig::builder().with_client_cert_verifier(verifier)
            }
        };
        
        let mut config = builder
            .with_single_cert(certs, key)
            .map_err(TlsError::from_rustls_error)?;
        
        config.alpn_protocols = vec![
            #[cfg(feature = "http2")]
            b"h2".into(),
            b"http/1.1".into(),
            b"http/1.0".into()
        ];
        
        Ok(config)
    }
    
    #[inline]
    fn load_cert_file<'a>(path: impl AsRef<Path>) -> Result<Vec<CertificateDer<'a>>> {
        CertificateDer::pem_file_iter(path)
            .map_err(TlsError::from_rustls_pem_error)?
            .collect::<std::result::Result<Vec<_>, _>>()
            .map_err(TlsError::from_rustls_pem_error)
    }
    
    #[inline]
    fn load_key_file<'a>(path: impl AsRef<Path>) -> Result<PrivateKeyDer<'a>> {
        PrivateKeyDer::from_pem_file(path).map_err(TlsError::from_rustls_pem_error)
    }

    fn read_trust_anchor(path: impl AsRef<Path>) -> Result<RootCertStore> {
        let trust_anchors = Self::load_cert_file(path)?;
        let mut store = RootCertStore::empty();
        let (added, _skipped) = store.add_parsable_certificates(trust_anchors);
        if added == 0 {
            return Err(TlsError::cert_parse_error());
        }
        Ok(store)
    }
}

struct TlsError;
impl TlsError {
    #[inline]
    fn from_rustls_error(error: tokio_rustls::rustls::Error) -> Error {
        Error::new(ErrorKind::Other, format!("TLS config error: {}", error))
    }

    #[inline]
    fn from_rustls_pem_error(error: tokio_rustls::rustls::pki_types::pem::Error) -> Error {
        Error::new(ErrorKind::Other, format!("TLS config error: {}", error))
    }

    #[inline]
    fn from_rustls_auth_error(error: tokio_rustls::rustls::server::VerifierBuilderError) -> Error {
        Error::new(ErrorKind::Other, format!("TLS config error: {}", error))
    }

    #[inline]
    fn cert_parse_error() -> Error {
        Error::new(ErrorKind::Other, "TLS config error: certificate parse error")
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
                    let http_result = next(ctx).await;

                    match http_result {
                        Ok(mut response) if !is_excluded(host.to_str().ok()) => {
                            response
                                .headers_mut()
                                .append(hsts_header, hsts_header_value.parse().unwrap());
                            Ok(response)
                        },
                        Ok(response) => Ok(response),
                        Err(error) => Err(error),
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
                    _ = tokio::time::sleep(Duration::from_secs(super::app::GRACEFUL_SHUTDOWN_TIMEOUT)) => (),
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
    fn serve_http_redirection(https_port: u16, stream: TcpStream, graceful_shutdown: &GracefulShutdown) {
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
}