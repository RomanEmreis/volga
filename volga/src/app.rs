//! Main application entry point

use self::pipeline::PipelineBuilder;
use hyper_util::rt::TokioIo;
use connection::Connection;
use crate::{
    http::request::request_body_limit::RequestBodyLimit,
    headers::cache_control::CacheControl,
    server::Server,
    Limit
};

use std::{
    future::Future,
    sync::{Arc, Weak}
};

use tokio::{
    io::self,
    net::{TcpListener, TcpStream},
    signal,
    sync::{watch, Semaphore}
};

#[cfg(feature = "rate-limiting")]
use {
    crate::rate_limiting::GlobalRateLimiter,
    std::{net::IpAddr, collections::HashSet},
};

#[cfg(any(
    feature = "decompression-brotli",
    feature = "decompression-gzip",
    feature = "decompression-zstd",
    feature = "decompression-full"
))]
use crate::middleware::decompress::DecompressionLimits;

#[cfg(feature = "di")]
use crate::di::ContainerBuilder;

#[cfg(feature = "tracing")]
use crate::tracing::TracingConfig;

#[cfg(feature = "middleware")]
use crate::http::cors::CorsRegistry;

#[cfg(feature = "jwt-auth")]
use crate::auth::bearer::BearerAuthConfig;

#[cfg(feature = "tls")]
use crate::tls::TlsConfig;

#[cfg(feature = "openapi")]
use crate::openapi::OpenApiState;

#[cfg(feature = "static-files")]
pub use self::host_env::HostEnv;

#[cfg(feature = "http2")]
pub use crate::limits::Http2Limits;

#[cfg(feature = "tls")]
pub(crate) use app_env::GRACEFUL_SHUTDOWN_TIMEOUT;

pub(crate) use app_env::AppEnv;

pub mod router;
pub(crate) mod pipeline;
pub(crate) mod scope;
mod connection;
mod app_env;
#[cfg(feature = "static-files")]
mod host_env;

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
    pub(super) cors: CorsRegistry,
    
    /// Web Server's Hosting Environment
    #[cfg(feature = "static-files")]
    pub(super) host_env: HostEnv,
    
    /// Bearer Token Authentication & Authorization configuration options
    #[cfg(feature = "jwt-auth")]
    pub(super) auth_config: Option<BearerAuthConfig>,
    
    /// Global rate limiter
    #[cfg(feature = "rate-limiting")]
    pub(super) rate_limiter: Option<GlobalRateLimiter>,

    /// Trusted proxies for rate limiting IP extraction
    #[cfg(feature = "rate-limiting")]
    pub(super) trusted_proxies: Option<HashSet<IpAddr>>,

    /// Request/Middleware pipeline builder
    pub(super) pipeline: PipelineBuilder,

    /// Default `Cache-Control`
    /// 
    /// Default: `None`
    pub(super) cache_control: Option<CacheControl>,

    /// HTTP/2 resource and backpressure limits.
    #[cfg(feature = "http2")]
    pub(super) http2_limits: Http2Limits,

    /// Limits for decompression middleware
    #[cfg(any(
        feature = "decompression-brotli",
        feature = "decompression-gzip",
        feature = "decompression-zstd",
        feature = "decompression-full"
    ))]
    pub(super) decompression_limits: DecompressionLimits,

    /// OpenAPI registry and configuration.
    #[cfg(feature = "openapi")]
    pub(super) openapi: OpenApiState,
    
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
    implicit_head: bool,

    /// Maximum total size of all HTTP request headers, in bytes.
    max_header_size: Limit<usize>,

    /// Maximum number of HTTP request headers.
    max_header_count: Limit<usize>,

    /// Maximum number of simultaneous TCP connections.
    max_connections: Limit<usize>,
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
            cors: Default::default(),
            #[cfg(feature = "static-files")]
            host_env: HostEnv::default(),
            #[cfg(feature = "jwt-auth")]
            auth_config: None,
            #[cfg(feature = "rate-limiting")]
            rate_limiter: None,
            #[cfg(feature = "rate-limiting")]
            trusted_proxies: None,
            pipeline: PipelineBuilder::new(),
            connection: Default::default(),
            body_limit: Default::default(),
            no_delay: false,
            implicit_head: true,
            max_header_count: Limit::Default,
            max_header_size: Limit::Default,
            max_connections: Limit::Default,
            cache_control: None,
            #[cfg(feature = "http2")]
            http2_limits: Default::default(),
            #[cfg(any(
                feature = "decompression-brotli",
                feature = "decompression-gzip",
                feature = "decompression-zstd",
                feature = "decompression-full"
            ))]
            decompression_limits: Default::default(),
            #[cfg(feature = "openapi")]
            openapi: Default::default(),
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
    /// # Parameters
    /// - `Limit::Default` — use the framework default (5 MB)
    /// - `Limit::Limited(n)` — enforce an explicit limit
    /// - `Limit::Unlimited` — disables the body size check completely
    /// 
    /// Default: 5 MB
    pub fn with_body_limit(mut self, limit: Limit<usize>) -> Self {
        self.body_limit = limit.into();
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
    /// When enabled, `HEAD` requests follow the same routing,
    /// validation, and authorization logic as `GET`, but must not
    /// produce a response body.
    ///
    /// Default: `true`
    pub fn without_implicit_head(mut self) -> Self {
        self.implicit_head = false;
        self
    }

    /// Sets the maximum allowed size of the HTTP/2 header list.
    ///
    /// This limit controls the total size (in bytes) of all headers for a single HTTP/2 request.
    ///
    /// # Parameters
    /// - `limit` — a [`Limit<u32>`]:
    ///   - `Limit::Default` — uses the framework default (recommended)
    ///   - `Limit::Limited(n)` — enforces an explicit upper bound
    ///   - `Limit::Unlimited` — treated as `u32::MAX` in production; 
    ///     in debug builds this will **panic** to catch misconfiguration early.
    pub fn with_max_header_list_size(mut self, size: Limit<usize>) -> Self {
        self.max_header_size = match size {
            Limit::Limited(size) => {
                assert!(u32::try_from(size).is_ok(), "header limit too big");
                Limit::Limited(size)
            },

            Limit::Unlimited => {
                #[cfg(debug_assertions)]
                panic!("HTTP/2 max_header_list_size cannot be Unlimited; use Limit::Limited(u32) instead");
            
                #[cfg(not(debug_assertions))]
                {
                    #[cfg(feature = "tracing")]
                    tracing::warn!(
                        "max_header_list_size set to Unlimited; using u32::MAX for production"
                    );

                    Limit::Limited(u32::MAX as usize)
                }
            },

            Limit::Default => Limit::Default
        };

        self
    }

    /// Sets the maximum allowed number of HTTP request headers.
    ///
    /// This limit is enforced in a protocol-aware manner:
    ///
    /// - For HTTP/1, the limit is applied by the HTTP/1 parser,
    ///   rejecting the request before it reaches application code.
    /// 
    /// - For HTTP/2, the limit is validated after headers are decoded,
    ///   using a middleware check.
    ///
    /// When exceeded, the request is rejected with
    /// `431 Request Header Fields Too Large`.
    ///
    /// If not set, the framework relies on the underlying HTTP implementation
    /// defaults.
    pub fn with_max_header_count(mut self, count: Limit<usize>) -> Self { 
        self.max_header_count = count;
        self
    }

    /// Sets the maximum number of concurrent TCP connections.
    ///
    /// This limit is applied at the transport level and acts as a
    /// fail-fast mechanism to protect the server under a high load.
    ///
    /// - `Default`: No explicit limit is enforced.
    /// - `Limited(n)`: At most `n` concurrent connections are allowed.
    /// - `Unlimited`: Disables connection limiting entirely.
    pub fn with_max_connections(mut self, count: Limit<usize>) -> Self {
        self.max_connections = count;
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
        let Some(runtime) = create_tokio_runtime() else {
            return;
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

    /// Starts the [`App`] using the custom [`std::net::TcpListener`] with its own Tokio runtime.
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
    /// use std::net::TcpListener;
    /// # fn docs() -> std::io::Result<()> {
    /// let app = App::new();
    /// let listener = TcpListener::bind("localhost:7878")?;
    /// 
    /// app.run_blocking_with_std_listener(listener);
    /// # Ok(())
    /// # }
    /// ```
    pub fn run_blocking_with_std_listener(self, tcp_listener: std::net::TcpListener) {
        let Some(runtime) = create_tokio_runtime() else {
            return;
        };

        runtime.block_on(async {
            if let Err(err) = self.run_with_std_listener(tcp_listener).await {
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
    pub async fn run(mut self) -> io::Result<()> {
        self.use_endpoints();
        
        let tcp_listener = TcpListener::bind(self.connection.socket).await?;
        self.run_internal(tcp_listener).await
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
    pub async fn run(self) -> io::Result<()> {
        let tcp_listener = TcpListener::bind(self.connection.socket).await?;
        self.run_internal(tcp_listener).await
    }

    /// Runs the [`App`] using the custom [`tokio::net::TcpListener`] in the current asynchronous runtime.
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
    /// use tokio::net::TcpListener;
    /// 
    /// #[tokio::main]
    /// async fn main() -> std::io::Result<()> {
    ///     let app = App::new();
    /// 
    ///     let listener = TcpListener::bind("localhost:7878").await?;
    ///     
    ///     app.run_with_listener(listener).await
    /// }
    /// ```
    ///
    /// # Errors
    /// Returns an `io::Error` if the server fails to start or encounters a fatal error.
    #[cfg(feature = "middleware")]
    pub fn run_with_listener(
        mut self, 
        tcp_listener: TcpListener
    ) -> impl Future<Output = io::Result<()>> {
        self.use_endpoints();

        self.run_internal(tcp_listener)
    }

    /// Runs the [`App`] using the custom [`tokio::net::TcpListener`] in the current asynchronous runtime.
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
    /// use tokio::net::TcpListener;
    ///
    /// #[tokio::main]
    /// async fn main() -> std::io::Result<()> {
    ///     let app = App::new();
    ///
    ///     let listener = TcpListener::bind("localhost:7878").await?;
    ///     
    ///     app.run_with_listener(listener).await
    /// }
    /// ```
    ///
    /// # Errors
    /// Returns an `io::Error` if the server fails to start or encounters a fatal error.
    #[cfg(not(feature = "middleware"))]
    pub fn run_with_listener(
        self,
        tcp_listener: TcpListener
    ) -> impl Future<Output = io::Result<()>> {
        self.run_internal(tcp_listener)
    }

    /// Runs the [`App`] using the custom [`std::net::TcpListener`] in the current asynchronous runtime.
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
    /// use std::net::TcpListener;
    ///
    /// #[tokio::main]
    /// async fn main() -> std::io::Result<()> {
    ///     let app = App::new();
    ///
    ///     let listener = TcpListener::bind("localhost:7878")?;
    ///     
    ///     app.run_with_std_listener(listener).await
    /// }
    /// ```
    ///
    /// # Errors
    /// Returns an `io::Error` if the server fails to start or encounters a fatal error.
    #[cfg(feature = "middleware")]
    pub async fn run_with_std_listener(
        mut self,
        tcp_listener: std::net::TcpListener
    ) -> io::Result<()> {
        self.use_endpoints();

        tcp_listener.set_nonblocking(true)?;
        let tcp_listener = TcpListener::from_std(tcp_listener)?;
        self.run_internal(tcp_listener).await
    }

    /// Runs the [`App`] using the custom [`std::net::TcpListener`] in the current asynchronous runtime.
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
    /// use std::net::TcpListener;
    ///
    /// #[tokio::main]
    /// async fn main() -> std::io::Result<()> {
    ///     let app = App::new();
    ///
    ///     let listener = TcpListener::bind("localhost:7878")?;
    ///     
    ///     app.run_with_std_listener(listener).await
    /// }
    /// ```
    ///
    /// # Errors
    /// Returns an `io::Error` if the server fails to start or encounters a fatal error.
    #[cfg(not(feature = "middleware"))]
    pub async fn run_with_std_listener(
        self,
        tcp_listener: std::net::TcpListener
    ) -> io::Result<()> {
        tcp_listener.set_nonblocking(true)?;
        let tcp_listener = TcpListener::from_std(tcp_listener)?;
        self.run_internal(tcp_listener).await
    }
    
    #[inline]
    async fn run_internal(self, tcp_listener: TcpListener) -> io::Result<()> {
        #[cfg(all(debug_assertions, feature = "openapi"))]
        if self.openapi.is_configure_but_not_exposed() { 
            #[cfg(feature = "tracing")]
            tracing::warn!("{}", crate::openapi::OPEN_API_NOT_EXPOSED_WARN);
            #[cfg(not(feature = "tracing"))]
            eprintln!("{}", crate::openapi::OPEN_API_NOT_EXPOSED_WARN);
        } 
        
        #[cfg(any(feature = "tls", feature = "tracing"))]
        let socket = tcp_listener.local_addr()?;
        
        let no_delay = self.no_delay;
        
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
        
        #[cfg(feature = "tls")]
        if let Some(redirection_config) = redirection_config 
            && redirection_config.enabled {
            Self::run_https_redirection_middleware(
                socket,
                redirection_config.http_port,
                shutdown_tx.clone());
        }

        let active_connections = self.active_connections();
        let app_instance: Arc<AppEnv> = Arc::new(self.try_into()?);

        loop {
            let (stream, _) = tokio::select! {
                Ok(connection) = tcp_listener.accept() => connection,
                _ = shutdown_tx.closed() => break,
            };

            let permit = match active_connections.as_ref() {
                Some(sem) => match sem.clone().try_acquire_owned() {
                    Ok(p) => Some(p),
                    Err(_) => {
                        #[cfg(feature = "tracing")]
                        tracing::warn!("incoming connection rejected: max_connections limit reached");
                        drop(stream);
                        continue;
                    }
                },
                None => None,
            };

            if let Err(_err) = stream.set_nodelay(no_delay) {
                #[cfg(feature = "tracing")]
                tracing::warn!("failed to set TCP_NODELAY on incoming connection: {_err:#}");
            }

            let instance = Arc::downgrade(&app_instance);
            tokio::spawn(async move {
                let _permit = permit;
                Self::handle_connection(stream, instance).await
            });
        }
    
        drop(tcp_listener);

        if let Some(app_instance) = Arc::into_inner(app_instance) {
            app_instance.shutdown().await;
        }
        Ok(())
    }
    
    #[inline]
    fn active_connections(&self) -> Option<Arc<Semaphore>> {
        match self.max_connections {
            Limit::Limited(n) => Some(
                Arc::new(Semaphore::new(n))
            ),
            _ => None,
        }
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
    async fn handle_connection(stream: TcpStream, app_instance: Weak<AppEnv>) {
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
        println!("│                >> Volga v{version:<5}                │");
        println!("│     Listening on: {url:<28}│");
        println!("╰───────────────────────────────────────────────╯\x1b[0m");
        
        let routes = self.pipeline
            .endpoints()
            .collect();
        println!("{routes}");
    }
}

#[inline]
fn create_tokio_runtime() -> Option<tokio::runtime::Runtime> {
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
            return None;
        }
    };

    Some(runtime)
}

#[cfg(test)]
mod tests {
    use std::net::SocketAddr;
    use crate::http::request::request_body_limit::RequestBodyLimit;
    use crate::{App, Limit};

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
        let app = App::new().with_body_limit(Limit::Limited(10));
        let RequestBodyLimit::Enabled(limit) = app.body_limit else { unreachable!() };

        assert_eq!(limit, 10)
    }

    #[test]
    fn it_disables_body_limit() {
        let app = App::new().without_body_limit();

        let RequestBodyLimit::Disabled = app.body_limit else { panic!() };
    }

    #[test]
    fn it_sets_max_headers_size_limit() {
        let app = App::new().with_max_header_list_size(Limit::Limited(1024));
        let Limit::Limited(limit) = app.max_header_size else { unreachable!() };

        assert_eq!(limit, 1024)
    }

    #[test]
    #[should_panic]
    fn it_panics_on_unlimited() {
        App::new().with_max_header_list_size(Limit::Unlimited);
    }

    #[test]
    #[should_panic]
    fn it_panics_on_large_value() {
        App::new().with_max_header_list_size(Limit::Limited(usize::MAX));
    }

    #[test]
    fn it_sets_max_headers_count_limit() {
        let app = App::new().with_max_header_count(Limit::Limited(10));
        let Limit::Limited(limit) = app.max_header_count else { unreachable!() };

        assert_eq!(limit, 10)
    }

    #[test]
    fn it_disables_implicit_head() {
        let app = App::new().without_implicit_head();

        assert!(!app.implicit_head)
    }

    #[test]
    fn it_sets_max_connections_limit() {
        let app = App::new().with_max_connections(Limit::Limited(1000));
        let Limit::Limited(limit) = app.max_connections else { unreachable!() };

        assert_eq!(limit, 1000)
    }
}