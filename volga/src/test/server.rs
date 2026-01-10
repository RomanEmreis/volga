//! Common test utilities

use crate::App;
use std::{net::TcpListener, fmt::{Debug, Formatter}};
use tokio::sync::oneshot;
#[cfg(feature = "ws")]
use {
    super::ws::TestWebSocket,
    crate::http::{Uri, Request, Method, HttpBody},
    crate::headers::SEC_WEBSOCKET_PROTOCOL,
    hyper_util::rt::{TokioExecutor, TokioIo},
    tokio_tungstenite::{WebSocketStream, tungstenite::{protocol, ClientRequestBuilder}},
    tokio::net::TcpStream,
    
};

type AppSetupFn = Box<dyn FnOnce(App) -> App + Send>;
type ServerSetupFn = Box<dyn FnOnce(&mut App) + Send>;

/// Builder for configuring and spawning a [`TestServer`].
///
/// This type allows fine-grained control over how the Volga application
/// is constructed before being started.
///
/// Configuration is split into two phases:
///
/// 1. Application-level configuration using [`configure`]
/// 2. Route and middleware setup using [`setup`]
///
/// This separation mirrors the lifecycle of a typical Volga application
/// and helps keep test setup clear and intention-revealing.
pub struct TestServerBuilder {
    is_https: bool,
    app_config: Option<AppSetupFn>,
    routes: Vec<ServerSetupFn>,
}

impl Debug for TestServerBuilder {
    #[inline]
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestServerBuilder(...)").finish()
    }
}

impl Default for TestServerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// A running Volga application instance intended for integration testing.
///
/// `TestServer` spawns a real HTTP server in the background, bound to a
/// randomly assigned local port, and provides utilities for interacting
/// with it using an HTTP client.
///
/// The server is shut down gracefully when:
/// - [`shutdown`](Self::shutdown) is called explicitly, or
/// - the `TestServer` instance is dropped.
///
/// # Lifecycle
///
/// - A `TestServer` is created using [`TestServer::builder`] or
///   [`TestServer::spawn`].
/// - The server runs concurrently with the test.
/// - Shutdown is cooperative and bounded by a timeout.
///
/// # Notes
///
/// - Each `TestServer` instance is fully isolated.
/// - No ports or global state are shared between tests.
///
/// # Example
///
/// ```no_run
/// use volga::test::TestServer;
///
/// #[tokio::test]
/// async fn example() {
///     let server = TestServer::spawn(|app| {
///         app.map_get("/ping", || async { "pong" });
///     })
///     .await;
///
///     let response = server
///         .client()
///         .get(server.url("/ping"))
///         .send()
///         .await
///         .unwrap();
///
///     assert!(response.status().is_success());
///     assert_eq!(response.text().await.unwrap(), "pong");
///
///     server.shutdown().await;
/// }
/// ```
#[derive(Debug)]
pub struct TestServer {
    /// Represents a randomly assigned free port that the test server is bound to.
    pub port: u16,
    is_https: bool,
    shutdown_tx: Option<oneshot::Sender<()>>,
    server_handle: Option<tokio::task::JoinHandle<()>>,
}

impl TestServerBuilder {
    /// Creates a new [`TestServerBuilder`]
    pub fn new() -> Self {
        Self {
            app_config: None,
            routes: Vec::new(),
            is_https: false
        }
    }

    /// Applies application-level configuration before routes are registered.
    ///
    /// This method is intended for configuring global application settings,
    /// such as middleware, CORS, or other top-level options.
    ///
    /// The provided function receives ownership of the [`App`] and must
    /// return the modified instance.
    pub fn configure<F>(mut self, config: F) -> Self
    where
        F: FnOnce(App) -> App + Send + 'static,
    {
        self.app_config = Some(Box::new(config));
        self
    }

    /// Registers routes and middleware on the test application.
    ///
    /// This method is typically used to define routes and attach middleware
    /// required for a specific test case.
    ///
    /// Multiple calls to `setup` are executed in the order they were added.
    pub fn setup<F>(mut self, f: F) -> Self
    where
        F: FnOnce(&mut App) + Send + 'static,
    {
        self.routes.push(Box::new(f));
        self
    }
    
    /// Configures whether to use HTTPS
    pub fn with_https(mut self) -> Self {
        self.is_https = true;
        self
    }

    /// Builds and starts the test server.
    ///
    /// This method spawns the server in the background and waits until it
    /// is ready to accept incoming connections.
    pub async fn build(self) -> TestServer {
        let port = TestServer::get_free_port();
        let (tx, rx) = oneshot::channel();
        let (ready_tx, ready_rx) = oneshot::channel();

        let app_config = self.app_config;
        let routes = self.routes;

        let server_handle = tokio::spawn(async move {
            let mut app = App::new()
                .bind(format!("127.0.0.1:{}", port))
                .with_no_delay()
                .without_greeter();

            if let Some(config) = app_config {
                app = config(app);
            }

            for route in routes {
                route(&mut app);
            }

            let _ = ready_tx.send(());

            tokio::select! {
                _ = app.run() => {},
                _ = rx => {}
            }
        });

        let _ = ready_rx.await;

        TestServer {
            port,
            is_https: self.is_https,
            shutdown_tx: Some(tx),
            server_handle: Some(server_handle),
        }
    }
}

impl TestServer {
    /// Creates a new [`TestServerBuilder`] for configuring a test server.
    ///
    /// This is the most flexible way to construct a `TestServer`, allowing
    /// customization of application configuration and route setup.
    ///
    /// See [`TestServerBuilder`] for details.
    #[inline]
    pub fn builder() -> TestServerBuilder {
        TestServerBuilder::new()
    }

    /// Spawns a test server using a simple setup function.
    ///
    /// This is a convenience method for cases where only route configuration
    /// is required and no application-level customization is needed.
    ///
    /// Equivalent to:
    ///
    /// ```rust,ignore
    /// TestServer::builder()
    ///     .setup(setup)
    ///     .build()
    ///     .await
    /// ```
    #[inline]
    pub async fn spawn<F>(setup: F) -> Self
    where
        F: FnOnce(&mut App) + Send + 'static,
    {
        TestServerBuilder::new()
            .setup(setup)
            .build()
            .await
    }

    /// Constructs an absolute URL for the given path.
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let url = server.url("/users");
    /// ```
    pub fn url(&self, path: &str) -> String {
        let protocol = if self.is_https { "https" } else { "http" };
        format!("{protocol}://127.0.0.1:{}{path}", self.port)
    }

    /// Creates an HTTP client builder configured for communicating with this server.
    ///
    /// The client is configured to match the server's HTTP protocol
    /// (HTTP/1.1 or HTTP/2) based on enabled features.
    pub fn client_builder(&self) -> reqwest::ClientBuilder {
        if cfg!(all(feature = "http1", not(feature = "http2"))) {
            reqwest::Client::builder().http1_only()
        } else {
            reqwest::Client::builder().http2_prior_knowledge()
        }
    }
    
    /// Creates an HTTP client configured for communicating with this server.
    ///
    /// The client is configured to match the server's HTTP protocol
    /// (HTTP/1.1 or HTTP/2) based on enabled features.
    pub fn client(&self) -> reqwest::Client {
        self.client_builder().build().unwrap()
    }

    /// Establishes a WebSocket connection without subprotocol negotiation.
    #[cfg(feature = "ws")]
    pub async fn ws(&self, path: &str) -> TestWebSocket {
        self.ws_with_protocols::<0>(path, []).await
    }

    /// Establishes a WebSocket connection with known subprotocols.
    #[cfg(feature = "ws")]
    pub async fn ws_with_protocols<const N: usize>(
        &self,
        path: &str,
        known_protocols: [&'static str; N],
    ) -> TestWebSocket {
        if cfg!(all(feature = "http1", not(feature = "http2"))) {
            self.ws_http1(path, known_protocols).await
        } else {
            self.ws_http2(path, known_protocols).await
        }
    }

    /// Gracefully shuts down the test server.
    ///
    /// This method signals the server to stop accepting new connections
    /// and waits for the background task to complete, up to a fixed timeout.
    ///
    /// Calling this method is optional; the server will also shut down
    /// automatically when dropped. However, calling it explicitly is
    /// recommended for deterministic test behavior.
    pub async fn shutdown(mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }

        if let Some(handle) = self.server_handle.take() {
            let _ = tokio::time::timeout(
                tokio::time::Duration::from_secs(5),
                handle
            ).await;
        }
    }

    /// Returns a free available port
    #[inline]
    pub fn get_free_port() -> u16 {
        TcpListener::bind("127.0.0.1:0")
            .unwrap()
            .local_addr()
            .unwrap()
            .port()
    }

    #[cfg(feature = "ws")]
    async fn ws_http1<const N: usize>(&self, path: &str, known_protocols: [&'static str; N]) -> TestWebSocket {
        let uri = format!("ws://127.0.0.1:{}{}", self.port, path);

        let req = ClientRequestBuilder::new(Uri::try_from(uri).unwrap())
            .with_header(SEC_WEBSOCKET_PROTOCOL.to_string(), known_protocols.join(","));
        let (ws, _) = tokio_tungstenite::connect_async(req)
            .await
            .expect("WebSocket handshake failed");

        TestWebSocket::from_http1(ws)
    }

    #[cfg(feature = "ws")]
    async fn ws_http2<const N: usize>(&self, path: &str, known_protocols: [&'static str; N]) -> TestWebSocket {
        let io = TokioIo::new(
            TcpStream::connect(format!("127.0.0.1:{}", self.port))
                .await
                .expect("Failed to connect to test server"),
        );

        let (mut sender, conn) =
            hyper::client::conn::http2::Builder::new(TokioExecutor::new())
                .handshake(io)
                .await
                .expect("HTTP/2 handshake failed");

        tokio::spawn(async move {
            let _ = conn.await;
        });
        
        let request = Request::builder()
            .method(Method::CONNECT)
            .extension(hyper::ext::Protocol::from_static("websocket"))
            .header(SEC_WEBSOCKET_PROTOCOL, known_protocols.join(","))
            .uri(path)
            .body(HttpBody::empty())
            .unwrap();
        
        let mut response = sender.send_request(request).await.unwrap();
        let upgraded = hyper::upgrade::on(&mut response).await.unwrap();

        let io = TokioIo::new(upgraded);
        let ws = WebSocketStream::from_raw_socket(
            io,
            protocol::Role::Client,
            None,
        )
        .await;

        TestWebSocket::from_http2(ws)
    }
}

impl Drop for TestServer {
    fn drop(&mut self) {
        if let Some(tx) = self.shutdown_tx.take() {
            let _ = tx.send(());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn it_starts_server_and_shuts_down() {
        let server = TestServer::builder().build().await;
        server.shutdown().await;
    }


    #[tokio::test]
    async fn it_binds_server_to_free_port() {
        let server = TestServer::builder().build().await;
        let resp = server.client()
            .get(server.url("/"))
            .send()
            .await
            .unwrap();
        
        assert_eq!(resp.status(), 404);
    }


    #[tokio::test]
    async fn it_drops_server_gracefully() {
        {
            let _server = TestServer::builder().build().await;
        } // drop here

        // test must finish
    }
}