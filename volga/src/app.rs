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

#[cfg(feature = "middleware")]
use crate::http::CorsConfig;

#[cfg(feature = "jwt-auth")]
use crate::auth::bearer::{BearerAuthConfig, BearerTokenService};

#[cfg(feature = "rate-limiting")]
use crate::rate_limiting::GlobalRateLimiter;

#[cfg(feature = "static-files")]
pub use self::env::HostEnv;

#[cfg(feature = "static-files")]
pub mod env;
pub mod router;
pub(crate) mod pipeline;
pub(crate) mod scope;

pub(super) const GRACEFUL_SHUTDOWN_TIMEOUT: u64 = 10;
const DEFAULT_PORT: u16 = 7878;

/// The main entry point for building and running a Volga application.
///
/// `App` is used to configure the HTTP server, define middleware, and register routes.
///
/// Once configured, the application can be started either asynchronously using [`App::run`],
/// or in a blocking context using [`App::run_blocking`].
///
/// **Note:** A _blocking context_ means that the current thread (typically `main`)
/// will wait until the application finishes running. Internally, however,
/// the application still runs asynchronously on the Tokio runtime.
/// 
/// # Async Example
/// ```no_run
/// use volga::App;
///
/// #[tokio::main]
/// async fn main() -> std::io::Result<()> {
///     let app = App::new().bind("127.0.0.1:7878");
///     app.run().await
/// }
/// ```
///
/// # Blocking Example
/// ```no_run
/// use volga::App;
///
/// let app = App::new().bind("127.0.0.1:7878");
/// app.run_blocking();
/// ```
#[derive(Debug)]
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

    /// CORS configuration options
    #[cfg(feature = "middleware")]
    pub(super) cors_config: Option<CorsConfig>,
    
    /// Web Server's Hosting Environment
    #[cfg(feature = "static-files")]
    pub(super) host_env: HostEnv,
    
    /// Bearer Token Authentication & Authorization configuration options
    #[cfg(feature = "jwt-auth")]
    pub(super) auth_config: Option<BearerAuthConfig>,
    
    /// Global rate limiter
    #[cfg(feature = "rate-limiting")]
    pub(super) rate_limiter: Option<GlobalRateLimiter>,

    /// Request/Middleware pipeline builder
    pub(super) pipeline: PipelineBuilder,
    
    /// TCP connection parameters
    connection: Connection,
    
    /// Request body limit
    /// 
    /// Default: 5 MB
    body_limit: RequestBodyLimit,
    
    /// `TCP_NODELAY` flag
    /// 
    /// Default: `false`
    no_delay: bool,
    
    /// Determines whether to show a welcome screen
    /// 
    /// Default: `true`
    show_greeter: bool,

    /// Controls whether a `HEAD` route is automatically registered
    /// for this `GET` handler.
    ///
    /// When enabled, `HEAD` requests follow the same routing,
    /// validation, and authorization logic as `GET`, but must not
    /// produce a response body.
    ///
    /// Default: `true`
    implicit_head: bool
}

/// Wraps a socket
#[derive(Debug)]
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

/// Contains a shared resource of running Web Server
pub(crate) struct AppInstance {
    /// Incoming TLS connection acceptor
    #[cfg(feature = "tls")]
    pub(super) acceptor: Option<TlsAcceptor>,
    
    /// Dependency Injection container
    #[cfg(feature = "di")]
    container: Container,

    /// Web Server's Hosting Environment
    #[cfg(feature = "static-files")]
    pub(super) host_env: HostEnv,
    
    /// Service that validates/generates JWTs
    #[cfg(feature = "jwt-auth")]
    pub(super) bearer_token_service: Option<BearerTokenService>,

    /// Global rate limiter
    #[cfg(feature = "rate-limiting")]
    pub(super) rate_limiter: Option<Arc<GlobalRateLimiter>>,
    
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
        #[cfg(feature = "jwt-auth")]
        let bearer_token_service = app.auth_config.map(Into::into);
        
        let app_instance = Self {
            body_limit: app.body_limit,
            pipeline: app.pipeline.build(),
            graceful_shutdown: GracefulShutdown::new(),
            #[cfg(feature = "static-files")]
            host_env: app.host_env,
            #[cfg(feature = "di")]
            container: app.container.build(),
            #[cfg(feature = "rate-limiting")]
            rate_limiter: app.rate_limiter.map(Arc::new),
            #[cfg(feature = "jwt-auth")]
            bearer_token_service,
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
    /// Initializes a new instance of the [`App`] which will be bound to the 127.0.0.1:7878 socket by default.
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
            #[cfg(feature = "middleware")]
            cors_config: None,
            #[cfg(feature = "static-files")]
            host_env: HostEnv::default(),
            #[cfg(feature = "jwt-auth")]
            auth_config: None,
            #[cfg(feature = "rate-limiting")]
            rate_limiter: None,
            pipeline: PipelineBuilder::new(),
            connection: Default::default(),
            body_limit: Default::default(),
            no_delay: false,
            implicit_head: true,
            #[cfg(debug_assertions)]
            show_greeter: true,
            #[cfg(not(debug_assertions))]
            show_greeter: false,
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
    
    ///Sets the value of the `TCP_NODELAY` option on this socket.
    /// 
    /// If set, this option disables the Nagle algorithm. 
    /// This means that segments are always sent as soon as possible, 
    /// even if there is only a small amount of data.
    /// When not set, data is buffered until there is a sufficient amount to send out, 
    /// thereby avoiding the frequent sending of small packets.
    pub fn with_no_delay(mut self) -> Self {
        self.no_delay = true;
        self
    }
    
    /// Disables a welcome message on start
    /// 
    /// Default: *enabled*
    pub fn without_greeter(mut self) -> Self {
        self.show_greeter = false;
        self
    }

    /// Disables automatic registration of a `HEAD` route
    /// for the `GET` handler.
    ///
    /// After calling this method, `HEAD` requests to the same
    /// route will result in `405 Method Not Allowed` unless a
    /// separate `HEAD` handler is explicitly registered.
    pub fn without_implicit_head(mut self) -> Self {
        self.implicit_head = false;
        self
    }

    /// Starts the [`App`] with its own Tokio runtime.
    ///
    /// This method is intended for simple use cases where you don't already have a Tokio runtime setup.
    /// Internally, it creates and runs a multi-threaded Tokio runtime to execute the application.
    ///
    /// **Note:** This method **must not** be called from within an existing Tokio runtime
    /// (e.g., inside an `#[tokio::main]` async function), or it will panic.
    /// If you are already using Tokio in your application, use [`App::run`] instead.
    ///
    /// # Example
    /// ```no_run
    /// use volga::App;
    ///
    ///  let app = App::new().bind("127.0.0.1:7878");
    ///  app.run_blocking();
    /// ```
    pub fn run_blocking(self) {
        if tokio::runtime::Handle::try_current().is_ok() {
            panic!("`App::run_blocking()` cannot be called inside an existing Tokio runtime. Use `run().await` instead.");
        }

        let runtime = match tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .build()
        {
            Ok(rt) => rt,
            Err(err) => {
                #[cfg(feature = "tracing")]
                tracing::error!("failed to start the runtime: {err:#}");
                #[cfg(not(feature = "tracing"))]
                eprintln!("failed to start the runtime: {err:#}");
                return;
            }
        };

        runtime.block_on(async {
            if let Err(err) = self.run().await {
                #[cfg(feature = "tracing")]
                tracing::error!("failed to run the server: {err:#}");
                #[cfg(not(feature = "tracing"))]
                eprintln!("failed to run the server: {err:#}");
            }
        });
    }

    /// Runs the [`App`] using the current asynchronous runtime.
    ///
    /// This method must be called inside an existing asynchronous context,
    /// typically from within a function annotated with `#[tokio::main]` or a manually started runtime.
    ///
    /// Unlike [`App::run_blocking`], this method does **not** create a runtime.
    /// It gives you full control over runtime configuration, task execution, and integration
    /// with other async components.
    ///
    /// # Example
    /// ```no_run
    /// use volga::App;
    ///
    /// #[tokio::main]
    /// async fn main() -> std::io::Result<()> {
    ///     let app = App::new().bind("127.0.0.1:7878");
    ///     app.run().await
    /// }
    /// ```
    ///
    /// # Errors
    /// Returns an `io::Error` if the server fails to start or encounters a fatal error.
    #[cfg(feature = "middleware")]
    pub fn run(mut self) -> impl Future<Output = io::Result<()>> {
        self.use_endpoints();
        self.run_internal()
    }

    /// Runs the [`App`] using the current asynchronous runtime.
    ///
    /// This method must be called inside an existing asynchronous context,
    /// typically from within a function annotated with `#[tokio::main]` or a manually started runtime.
    ///
    /// Unlike [`App::run_blocking`], this method does **not** create a runtime.
    /// It gives you full control over runtime configuration, task execution, and integration
    /// with other async components.
    ///
    /// # Example
    /// ```no_run
    /// use volga::App;
    ///
    /// #[tokio::main]
    /// async fn main() -> std::io::Result<()> {
    ///     let app = App::new().bind("127.0.0.1:7878");
    ///     app.run().await
    /// }
    /// ```
    ///
    /// # Errors
    /// Returns an `io::Error` if the server fails to start or encounters a fatal error.
    #[cfg(not(feature = "middleware"))]
    pub fn run(self) -> impl Future<Output = io::Result<()>> {
        self.run_internal()
    }
    
    #[inline]
    async fn run_internal(self) -> io::Result<()> {
        let socket = self.connection.socket;
        let no_delay = self.no_delay;
        let tcp_listener = TcpListener::bind(socket).await?;

        #[cfg(debug_assertions)]
        self.print_welcome();
        
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
            .map(|config| config.https_redirection_config);
        
        let app_instance: Arc<AppInstance> = Arc::new(self.try_into()?);
        
        #[cfg(feature = "tls")]
        if let Some(redirection_config) = redirection_config 
            && redirection_config.enabled {
            Self::run_https_redirection_middleware(
                socket,
                redirection_config.http_port,
                shutdown_tx.clone());
        }

        loop {
            let (stream, _) = tokio::select! {
                Ok(connection) = tcp_listener.accept() => connection,
                _ = shutdown_tx.closed() => break,
            };
            if let Err(_err) = stream.set_nodelay(no_delay) {
                #[cfg(feature = "tracing")]
                tracing::warn!("failed to set TCP_NODELAY on incoming connection: {_err:#}");
            }
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
                Err(err) => tracing::error!("unable to listen for shutdown signal: {err:#}"),
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
        let peer_addr = match stream.peer_addr() { 
            Ok(addr) => addr,
            Err(_err) => {
                #[cfg(feature = "tracing")]
                tracing::error!("failed to get peer address: {_err:#}");
                return;
            }
        };
        
        #[cfg(not(feature = "tls"))]
        Server::new(TokioIo::new(stream), peer_addr).serve(app_instance).await;
        
        #[cfg(feature = "tls")]
        if let Some(acceptor) = app_instance.upgrade().and_then(|app| app.acceptor()) {
            let stream = match acceptor.accept(stream).await {
                Ok(tls_stream) => tls_stream,
                Err(_err) => {
                    #[cfg(feature = "tracing")]
                    tracing::error!("failed to perform tls handshake: {_err:#}");
                    return;
                }
            };
            let io = TokioIo::new(stream);
            Server::new(io, peer_addr).serve(app_instance).await;
        } else {
            let io = TokioIo::new(stream);
            Server::new(io, peer_addr).serve(app_instance).await;
        };
    }

    #[cfg(debug_assertions)]
    fn print_welcome(&self) {
        if !self.show_greeter {
            return;
        }

        let version = env!("CARGO_PKG_VERSION");
        let addr = self.connection.socket;

        #[cfg(not(feature = "tls"))]
        let url = format!("http://{addr}");
        #[cfg(feature = "tls")]
        let url = if self.tls_config.is_some() {
            format!("https://{addr}")
        } else {
            format!("http://{addr}")
        };

        println!();
        println!("\x1b[1;34m╭───────────────────────────────────────────────╮");
        println!("│          🚀 Welcome to Volga v{version:<5}           │");
        println!("│     Listening on: {url:<28}│");
        println!("╰───────────────────────────────────────────────╯\x1b[0m");
        
        let routes = self.pipeline
            .endpoints()
            .collect();
        println!("{routes}");
    }
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;
    use crate::http::request::request_body_limit::RequestBodyLimit;
    use crate::App;
    use crate::app::{AppInstance, Connection};

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
    fn it_creates_default_connection_from_empty_str() {
        let connection: Connection = "".into();

        #[cfg(target_os = "windows")]
        assert_eq!(connection.socket, SocketAddr::from(([127, 0, 0, 1], 7878)));
        #[cfg(not(target_os = "windows"))]
        assert_eq!(connection.socket, SocketAddr::from(([0, 0, 0, 0], 7878)));
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

    #[test]
    fn it_sets_default_body_limit() {
        let app = App::new();
        let RequestBodyLimit::Enabled(limit) = app.body_limit else { unreachable!() };

        assert_eq!(limit, 5242880)
    }

    #[test]
    fn it_sets_body_limit() {
        let app = App::new().with_body_limit(10);
        let RequestBodyLimit::Enabled(limit) = app.body_limit else { unreachable!() };

        assert_eq!(limit, 10)
    }

    #[test]
    fn it_disables_body_limit() {
        let app = App::new().without_body_limit();

        let RequestBodyLimit::Disabled = app.body_limit else { panic!() };
    }
    
    #[test]
    fn it_converts_into_app_instance() {
        let app = App::default();
        
        let app_instance: AppInstance = app.try_into().unwrap();
        let RequestBodyLimit::Enabled(limit) = app_instance.body_limit else { unreachable!() };

        assert_eq!(limit, 5242880);
    }

    #[test]
    fn it_debugs_connection() {
        let connection: Connection = ([127, 0, 0, 1], 5000).into();

        assert_eq!(format!("{connection:?}"), "Connection { socket: 127.0.0.1:5000 }");
    }
}