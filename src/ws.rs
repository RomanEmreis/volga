use std::future::Future;
use crate::{App, error::Error};
use crate::http::IntoResponse;
use crate::http::endpoints::{
    args::FromRequest,
    handlers::GenericHandler
};

pub use self::{
    connection::WebSocketConnection,
    websocket::WebSocket,
    args::{FromMessage, IntoMessage}
};

pub mod connection;
pub mod websocket;
pub mod args;

const UPGRADE: &str = "upgrade"; 
const VERSION: &str = "13";
const WEBSOCKET: &str = "websocket";
const WEBSOCKET_GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

pub(super) struct WebSocketError;

impl From<tokio_tungstenite::tungstenite::Error> for Error {
    #[inline]
    fn from(err: tokio_tungstenite::tungstenite::Error) -> Self {
        Error::server_error(err)
    }
}

impl WebSocketError {
    #[inline]
    fn invalid_upgrade_header() -> Error {
        Error::client_error("WebSocket error: invalid or missing \"Upgrade\" header")
    }

    #[inline]
    fn invalid_connection_header() -> Error {
        Error::client_error("WebSocket error: invalid or missing \"Connection\" header")
    }

    #[inline]
    fn invalid_version_header() -> Error {
        Error::client_error("WebSocket error: invalid or missing \"Sec-WebSocket-Version\" header")
    }

    #[inline]
    fn invalid_method(method: &crate::http::Method) -> Error {
        Error::client_error(format!("WebSocket error: request method must be {method}"))
    }

    #[inline]
    fn websocket_key_missing() -> Error {
        Error::client_error("WebSocket error: missing \"Sec-WebSocket-Key\" header")
    }

    #[inline]
    fn not_upgradable_connection() -> Error {
        Error::client_error("WebSocket error: connection is not upgradable")
    }
}

impl App {
    /// Adds a `handler` that has to be called when a connection upgrade requested. 
    /// That's usually happens when establishing a WebSocket connection.
    /// 
    /// # Example
    /// ```no_run
    /// use volga::{App, ws::{WebSocketConnection, WebSocket}};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    /// 
    /// app.map_ws("/ws", |conn: WebSocketConnection| async {
    ///     conn.on(|ws: WebSocket| async move {
    ///         // handle WebSocket connection 
    ///     })
    /// });
    ///# app.run().await
    ///# }
    /// ```
    pub fn map_ws<F, R, Args>(&mut self, pattern: &str, handler: F) -> &mut Self
    where
        F: GenericHandler<Args, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequest + Send + Sync + 'static
    {
        #[cfg(all(
            feature = "http1",
            not(feature = "http2"
        )))]
        self.map_get(pattern, handler);

        #[cfg(any(
            all(feature = "http1", feature = "http2"),
            all(feature = "http2", not(feature = "http1"))
        ))]
        self.map_connect(pattern, handler);

        self
    }

    /// Adds a `handler` that has to be called when a WebSocket connection is established
    /// 
    /// # Example
    /// ```no_run
    /// use volga::{App, ws::WebSocket};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    /// 
    /// app.map_ws1("/ws", |ws: WebSocket| async {
    ///     // handle WebSocket connection
    /// });
    ///# app.run().await
    ///# }
    /// ```
    pub fn map_ws1<F, Fut>(&mut self, pattern: &str, handler: F) -> &mut Self
    where
        F: FnOnce(WebSocket) -> Fut + Clone + Send + Sync + 'static,
        Fut: Future<Output = ()> + Send + 'static
    {
        self.map_ws(pattern, move |upgrade: WebSocketConnection| {
            let handler = handler.clone();
            async move { upgrade.on(handler) }
        })
    }

    /// Adds a `handler` that has to be called when a message received 
    /// from a client over WebSocket protocol
    /// 
    /// # Example
    /// ```no_run
    /// use volga::{App, ws::WebSocket};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    /// 
    /// app.map_msg("/ws", |msg: String| async {
    ///     format!("received msg: {}", msg)
    /// });
    ///# app.run().await
    ///# }
    /// ```
    pub fn map_msg<F, M, R, Fut>(&mut self, pattern: &str, handler: F) -> &mut Self 
    where
        F: Fn(M) -> Fut + Clone + Send + Sync + 'static,
        M: FromMessage + Send,
        R: IntoMessage + Send,
        Fut: Future<Output = R> + Send + 'static
    {
        self.map_ws1(pattern, move |mut ws: WebSocket| {
            let handler = handler.clone();
            async move { ws.on_msg(handler).await; }
        })
    }
}