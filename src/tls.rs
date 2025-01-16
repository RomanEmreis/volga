﻿use crate::{
    app::{App, AppInstance, scope::Scope},
    server::TlsServer
};

use std::{
    io::{Result, Error, ErrorKind},
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::Arc
};

use hyper_util::rt::TokioIo;

use tokio::net::{TcpListener, TcpStream};
use tokio::sync::broadcast;
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
    server::TlsStream,
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

pub struct TlsConfig {
    pub cert: PathBuf,
    pub key: PathBuf,
    pub use_https_redirection: bool,
    client_auth: ClientAuth,
}

enum ClientAuth {
    None,
    Optional(PathBuf),
    Required(PathBuf)
}

impl Default for TlsConfig {
    fn default() -> Self {
        let path = std::env::current_dir().unwrap_or_default();
        let cert = path.join(CERT_FILE_NAME);
        let key = path.join(KEY_FILE_NAME);
        Self { 
            key, 
            cert, 
            client_auth: ClientAuth::None,
            use_https_redirection: false
        }
    }
}

impl TlsConfig {
    /// Creates a configuration by loading cert and key files with default names from specified folder
    pub fn from_pem(path: impl AsRef<Path>) -> Self {
        let path = path.as_ref();
        let cert = path.join(CERT_FILE_NAME);
        let key = path.join(KEY_FILE_NAME);
        Self { 
            key, 
            cert, 
            client_auth: 
            ClientAuth::None,
            use_https_redirection: false,
        }
    }

    /// Creates a configuration by specifying path to cert and key files specifically
    pub fn from_pem_files(cert_file_path: &str, key_file_path: &str) -> Self {
        Self { 
            key: key_file_path.into(), 
            cert: cert_file_path.into(),
            client_auth: ClientAuth::None,
            use_https_redirection: false,
        }
    }
    
    /// Configures the trust anchor for optional TLS client authentication.
    pub fn with_optional_client_auth(mut self, path: impl AsRef<Path>) -> Self {
        self.client_auth = ClientAuth::Optional(path.as_ref().into());
        self
    }

    /// Configures the trust anchor for required TLS client authentication.
    pub fn with_required_client_auth(mut self, path: impl AsRef<Path>) -> Self {
        self.client_auth = ClientAuth::Required(path.as_ref().into());
        self
    }

    /// Configures web server to redirect HTTP requests to HTTPS
    pub fn with_https_redirection(mut self) -> Self {
        self.use_https_redirection = true;
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
    pub fn use_tls(&mut self, config: TlsConfig) -> &mut Self {
        self.tls_config = Some(config);
        self
    }
    
    pub(super) fn run_https_redirection_middleware(socket: SocketAddr, mut shutdown_signal: broadcast::Receiver<()>) {
        tokio::spawn(async move {
            let https_port = socket.port();
            let http_port = https_port + 1;
            let socket = SocketAddr::new(socket.ip(), http_port);
            println!("Start redirecting to HTTPS from {socket}");
            
            if let Ok(tcp_listener) = TcpListener::bind(socket).await {
                loop {
                    tokio::select! {
                        Ok((stream, _)) = tcp_listener.accept() => {
                            tokio::spawn(Self::serve_http_redirection(https_port, stream));
                        }
                        _ = shutdown_signal.recv() => {
                            println!("Stop redirecting to HTTPS");
                            break;
                        }
                    }
                }
            } else {
                eprintln!("Unable to start HTTPS redirection listener");
            }
        });
    }

    #[inline]
    pub(super) async fn serve_tls(io: TokioIo<TlsStream<TcpStream>>, app_instance: Arc<AppInstance>) {
        let server = TlsServer::new(io);
        let scope = Scope::new(app_instance);

        server.serve(scope).await;
    }
    
    #[inline]
    async fn serve_http_redirection(https_port: u16, stream: TcpStream) {
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

        if let Err(err) = connection.await {
            eprintln!("Error serving connection: {:?}", err);
        }
    }
}

