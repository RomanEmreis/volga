use crate::{App, error::Error, HttpRequest};
use crate::http::IntoResponse;
use crate::http::endpoints::{
    args::{FromRequest, FromPayload, Payload},
    handlers::GenericHandler
};

pub use self::{
    connection::WebSocketConnection,
    websocket::WebSocket,
    args::{
        FromMessage, 
        IntoMessage, 
        MessageHandler, 
        WebSocketHandler
    }
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
    fn websocket_key_missing() -> Error {
        Error::client_error("WebSocket error: missing \"Sec-WebSocket-Key\" header")
    }

    #[inline]
    fn not_upgradable_connection() -> Error {
        Error::client_error("WebSocket error: connection is not upgradable")
    }
}

impl App {
    /// Adds a `handler` that has to be called when a bidirectional connection to WebSocket 
    /// or WebTransport protocol is requested.
    /// 
    /// # Example
    /// ```no_run
    /// use volga::{App, ws::{WebSocketConnection, WebSocket}};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    /// 
    /// app.map_conn("/ws", |conn: WebSocketConnection| async {
    ///     
    ///     // extract HTTP metadata, DI, etc.
    /// 
    ///     conn.on(|ws: WebSocket| async move {
    ///         // handle WebSocket connection 
    ///     })
    /// });
    ///# app.run().await
    ///# }
    /// ```
    pub fn map_conn<F, R, Args>(&mut self, pattern: &str, handler: F) -> &mut Self
    where
        F: GenericHandler<Args, Output = R>,
        R: IntoResponse + 'static,
        Args: FromRequest + Send + Sync + 'static
    {
        // Using GET for WebSocket protocol and HTTP/1
        #[cfg(all(
            feature = "http1",
            not(feature = "http2"
        )))]
        self.map_get(pattern, handler);

        // Using CONNECT for WebTransport protocol and HTTP/2
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
    /// app.map_ws("/ws", |ws: WebSocket| async {
    ///     // handle WebSocket connection
    /// });
    ///# app.run().await
    ///# }
    /// ```
    pub fn map_ws<F, Args>(&mut self, pattern: &str, handler: F) -> &mut Self
    where
        F: WebSocketHandler<Args, Output = ()>,
        Args: FromRequest + Send + Sync + 'static,
    {
        self.map_conn(pattern, move |req: HttpRequest| {
            let handler = handler.clone();
            async move {
                let (parts, body) = req.into_parts();
                let conn = WebSocketConnection::from_payload(Payload::Parts(&parts)).await?;
                let args = Args::from_request(HttpRequest::from_parts(parts, body)).await?;
                conn.on(move |ws| handler.call(ws, args))
            }
        })
    }

    /// Adds a `handler` that has to be called when a message received 
    /// from a client over WebSocket protocol
    /// 
    /// Note: In case of need to extract something, e.g. from DI, it must implement `Clone`.
    /// 
    /// # Example
    /// ```no_run
    /// use volga::{App, ws::WebSocket};
    ///
    ///# #[tokio::main]
    ///# async fn main() -> std::io::Result<()> {
    /// let mut app = App::new();
    /// 
    /// app.map_msg("/ws", |msg: String| async move {
    ///     format!("received msg: {}", msg)
    /// });
    ///# app.run().await
    ///# }
    /// ```
    pub fn map_msg<F, M, Args, R>(&mut self, pattern: &str, handler: F) -> &mut Self 
    where
        F: MessageHandler<M, Args, Output = R> + 'static,
        Args: FromRequest + Clone + Send + Sync + 'static,
        M: FromMessage + Send,
        R: IntoMessage + Send
    {
        self.map_conn(pattern, move |req: HttpRequest| {
            let handler = handler.clone();
            async move {
                let (parts, body) = req.into_parts();
                let conn = WebSocketConnection::from_payload(Payload::Parts(&parts)).await?;
                let args = Args::from_request(HttpRequest::from_parts(parts, body)).await.unwrap();
                conn.on(|mut ws| async move {
                    ws.on_msg(move |msg: M| handler.call(msg, args.clone())).await;
                })
            }
        })
    }
}
