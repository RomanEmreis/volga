//! Main application entry point

use self::pipeline::{Pipeline, PipelineBuilder};
use hyper_util::{rt::TokioIo, server::graceful::GracefulShutdown};
use std::net::IpAddr;

use crate::{
    http::request::request_body_limit::RequestBodyLimit,
    server::Server
};

use std::{
    future::Future,
    io::Error,
    net::SocketAddr,
    sync::{Arc, Weak}
};

use tokio::{
    io::self,
    net::{TcpListener, TcpStream},
    signal,
    sync::watch
};

#[cfg(feature = "di")]
use crate::di::{Container, ContainerBuilder};

#[cfg(feature = "tls")]
use tokio_rustls::TlsAcceptor;

#[cfg(feature = "tls")]
use crate::tls::TlsConfig;

#[cfg(feature = "tracing")]
use crate::tracing::TracingConfig;

pub mod router;
pub(crate) mod pipeline;
pub(crate) mod scope;

pub(super) const GRACEFUL_SHUTDOWN_TIMEOUT: u64 = 10;
const DEFAULT_PORT: u16 = 7878;

/// The web application used to configure the HTTP pipeline, and routes.
///
/// # Examples
/// ```no_run
///use volga::App;
///
///#[tokio::main]
///async fn main() -> std::io::Result<()> {
///    let mut app = App::new().bind("127.0.0.1:8080");
///    
///    app.run().await
///}
/// ```
pub struct App {
    /// Dependency Injection container builder
    #[cfg(feature = "di")]
    pub(super) container: ContainerBuilder,
    
    /// TLS configuration options
    #[cfg(feature = "tls")]
    pub(super) tls_config: Option<TlsConfig>,
    
    /// Tracing configuration options
    #[cfg(feature = "tracing")]
    pub(super) tracing_config: Option<TracingConfig>,
    
    /// Request/Middleware pipeline builder
    pub(super) pipeline: PipelineBuilder,
    
    /// TCP connection parameters
    connection: Connection,
    
    /// Request body limit
    /// 
    /// Default: 5 MB
    body_limit: RequestBodyLimit
}

/// Wraps a socket
pub struct Connection {
    socket: SocketAddr
}

impl Default for Connection {
    fn default() -> Self {
        #[cfg(target_os = "windows")]
        let ip = [127, 0, 0, 1];
        #[cfg(not(target_os = "windows"))]
        let ip = [0, 0, 0, 0];
        let socket = (ip, DEFAULT_PORT).into();
        Self { socket }
    }
}

impl From<&str> for Connection {
    fn from(s: &str) -> Self {
        if let Ok(socket) = s.parse::<SocketAddr>() {
            Self { socket }
        } else {
            Self::default()
        }
    }
}

impl<I: Into<IpAddr>> From<(I, u16)> for Connection {
    fn from(value: (I, u16)) -> Self {
        Self { socket: SocketAddr::from(value) }
    }
}

/// Contains a shared resources of running Web Server
pub(crate) struct AppInstance {
    /// Incoming TLS connection acceptor
    #[cfg(feature = "tls")]
    pub(super) acceptor: Option<TlsAcceptor>,
    
    /// Dependency Injection container
    #[cfg(feature = "di")]
    container: Container,
    
    /// Graceful shutdown utilities
    pub(super) graceful_shutdown: GracefulShutdown,
    
    /// Request body limit
    pub(super) body_limit: RequestBodyLimit,
    
    /// Request/Middleware pipeline
    pipeline: Pipeline,
}

impl TryFrom<App> for AppInstance {
    type Error = Error;

    fn try_from(app: App) -> Result<Self, Self::Error> {
        #[cfg(feature = "tls")]
        let acceptor = {
            let tls_config = app.tls_config
                .map(|config| config.build())
                .transpose()?;
            tls_config
                .map(|config| TlsAcceptor::from(Arc::new(config)))
        };
        let app_instance = Self {
            body_limit: app.body_limit,
            pipeline: app.pipeline.build(),
            graceful_shutdown: GracefulShutdown::new(),
            #[cfg(feature = "di")]
            container: app.container.build(),
            #[cfg(feature = "tls")]
            acceptor
        };
        Ok(app_instance)
    }
}

impl AppInstance {
    /// Gracefully shutdown current instance
    #[inline]
    async fn shutdown(self) {
        tokio::select! {
            _ = self.graceful_shutdown.shutdown() => {
                #[cfg(feature = "tracing")]
                tracing::info!("shutting down the server...");
            },
            _ = tokio::time::sleep(std::time::Duration::from_secs(GRACEFUL_SHUTDOWN_TIMEOUT)) => {
                #[cfg(feature = "tracing")]
                tracing::warn!("timed out wait for all connections to close");
            }
        }
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

/// General impl
impl App {
    /// Initializes a new instance of the `App` which will be bound to the 127.0.0.1:7878 socket by default.
    /// 
    ///# Examples
    /// ```no_run
    /// use volga::App;
    ///
    /// let app = App::new();
    /// ```
    pub fn new() -> Self {
        Self {
            #[cfg(feature = "di")]
            container: ContainerBuilder::new(),
            #[cfg(feature = "tls")]
            tls_config: None,
            #[cfg(feature = "tracing")]
            tracing_config: None,
            pipeline:PipelineBuilder::new(),
            connection: Default::default(),
            body_limit: Default::default(),
        }
    }

    /// Binds the `App` to the specified `socket` address.
    /// 
    ///# Examples
    /// ```no_run
    ///use volga::App;
    ///
    ///let app = App::new().bind("127.0.0.1:7878");
    ///let app = App::new().bind(([127,0,0,1], 7878));
    /// ```
    pub fn bind<S: Into<Connection>>(mut self, socket: S) -> Self {
        self.connection = socket.into();
        self
    }
    
    /// Sets a specific HTTP request body limit (in bytes)
    /// 
    /// Default: 5 MB
    pub fn with_body_limit(mut self, limit: usize) -> Self {
        self.body_limit = RequestBodyLimit::Enabled(limit);
        self
    }
    
    /// Disables a request body limit
    pub fn without_body_limit(mut self) -> Self {
        self.body_limit = RequestBodyLimit::Disabled;
        self
    }

    /// Runs the `App`
    #[cfg(feature = "middleware")]
    pub fn run(mut self) -> impl Future<Output = io::Result<()>> {
        self.use_endpoints();
        self.run_internal()
    }

    /// Runs the `App`
    #[cfg(not(feature = "middleware"))]
    pub fn run(self) -> impl Future<Output = io::Result<()>> {
        self.run_internal()
    }
    
    #[inline]
    async fn run_internal(self) -> io::Result<()> {
        let socket = self.connection.socket;
        let tcp_listener = TcpListener::bind(socket).await?;
        #[cfg(feature = "tracing")]
        {
            #[cfg(feature = "tls")]
            if self.tls_config.is_some() { 
                tracing::info!("listening on: https://{socket}")
            } else { 
                tracing::info!("listening on: http://{socket}") 
            };
            #[cfg(not(feature = "tls"))]
            tracing::info!("listening on: http://{socket}");
        }

        let (shutdown_tx, shutdown_rx) = watch::channel::<()>(());
        let shutdown_tx = Arc::new(shutdown_tx);
        Self::shutdown_signal(shutdown_rx);

        #[cfg(feature = "tls")]
        let redirection_config = self.tls_config
            .as_ref()
            .map(|config| config.https_redirection_config.clone());
        
        let app_instance: Arc<AppInstance> = Arc::new(self.try_into()?);
        
        #[cfg(feature = "tls")]
        if let Some(redirection_config) = redirection_config {
            if redirection_config.enabled {
                Self::run_https_redirection_middleware(
                    socket,
                    redirection_config.http_port,
                    shutdown_tx.clone());
            }
        }

        loop {
            let (stream, _) = tokio::select! {
                Ok(connection) = tcp_listener.accept() => connection,
                _ = shutdown_tx.closed() => break,
            };
            
            let instance = Arc::downgrade(&app_instance);
            tokio::spawn(Self::handle_connection(stream, instance));
        }
    
        drop(tcp_listener);

        if let Some(app_instance) = Arc::into_inner(app_instance) {
            app_instance.shutdown().await;
        }
        Ok(())
    }
    
    #[inline]
    fn shutdown_signal(shutdown_rx: watch::Receiver<()>) {
        tokio::spawn(async move {
            match signal::ctrl_c().await {
                Ok(_) => (),
                #[cfg(feature = "tracing")]
                Err(err) => tracing::error!("unable to listen for shutdown signal: {}", err),
                #[cfg(not(feature = "tracing"))]
                Err(_) => ()
            }
            #[cfg(feature = "tracing")]
            tracing::trace!("shutdown signal received, not accepting new requests");
            drop(shutdown_rx); 
        });
    }

    #[inline]
    async fn handle_connection(stream: TcpStream, app_instance: Weak<AppInstance>) {
        #[cfg(not(feature = "tls"))]
        Server::new(TokioIo::new(stream)).serve(app_instance).await;
        
        #[cfg(feature = "tls")]
        if let Some(acceptor) = app_instance.upgrade().and_then(|app| app.acceptor()) {
            let stream = match acceptor.accept(stream).await {
                Ok(tls_stream) => tls_stream,
                Err(err) => {
                    #[cfg(feature = "tracing")]
                    tracing::error!("failed to perform tls handshake: {err:#}");
                    return;
                }
            };
            let io = TokioIo::new(stream);
            Server::new(io).serve(app_instance).await;
        } else {
            let io = TokioIo::new(stream);
            Server::new(io).serve(app_instance).await;
        };
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;
    use crate::App;
    use crate::app::Connection;

    #[test]
    fn it_creates_connection_with_default_socket() {
        let connection = Connection::default();

        #[cfg(target_os = "windows")]
        assert_eq!(connection.socket, SocketAddr::from(([127, 0, 0, 1], 7878)));
        #[cfg(not(target_os = "windows"))]
        assert_eq!(connection.socket, SocketAddr::from(([0, 0, 0, 0], 7878)));
    }

    #[test]
    fn it_creates_connection_with_specified_socket() {
        let connection: Connection = "127.0.0.1:5000".into();

        assert_eq!(connection.socket, SocketAddr::from(([127, 0, 0, 1], 5000)));
    }

    #[test]
    fn it_creates_connection_with_specified_socket_from_tuple() {
        let connection: Connection = ([127, 0, 0, 1], 5000).into();

        assert_eq!(connection.socket, SocketAddr::from(([127, 0, 0, 1], 5000)));
    }
    
    #[test]
    fn it_creates_app_with_default_socket() {
        let app = App::new();
        
        #[cfg(target_os = "windows")]
        assert_eq!(app.connection.socket, SocketAddr::from(([127, 0, 0, 1], 7878)));
        #[cfg(not(target_os = "windows"))]
        assert_eq!(app.connection.socket, SocketAddr::from(([0, 0, 0, 0], 7878)));
    }

    #[test]
    fn it_binds_app_to_socket() {
        let app = App::new().bind("127.0.0.1:5001");

        assert_eq!(app.connection.socket, SocketAddr::from(([127, 0, 0, 1], 5001)));
    }
}