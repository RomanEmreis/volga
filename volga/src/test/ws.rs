//! Test utilities for working with WebSocket connections.
//!
//! This module provides [`TestWebSocket`] — a small abstraction used in tests
//! to interact with WebSocket endpoints regardless of the underlying HTTP
//! protocol (HTTP/1.1 or HTTP/2).
//!
//! The goal is to expose a **uniform, ergonomic API** for sending and receiving
//! WebSocket messages in integration tests, without leaking protocol-specific
//! details into test code.

use std::fmt::Debug;
use futures_util::{SinkExt, StreamExt};
use hyper::upgrade::Upgraded;
use hyper_util::rt::TokioIo;
use tokio::net::TcpStream;
use tokio_tungstenite::{WebSocketStream, MaybeTlsStream, tungstenite::Message};

/// Internal enum representing a WebSocket connection established
/// over different HTTP protocols.
///
/// This type is an implementation detail and is not exposed publicly.
/// It allows [`TestWebSocket`] to unify HTTP/1.1 and HTTP/2 WebSocket
/// handling behind a single API.
enum InnerWebSocket {
    /// WebSocket established over HTTP/1.1 using `tokio-tungstenite`.
    Http1(WebSocketStream<MaybeTlsStream<TcpStream>>),

    /// WebSocket established over HTTP/2 via `hyper::upgrade`.
    Http2(WebSocketStream<TokioIo<Upgraded>>),
}

/// A test-friendly wrapper around a WebSocket connection.
///
/// `TestWebSocket` provides a minimal, protocol-agnostic API for interacting
/// with WebSocket endpoints in tests. It abstracts away whether the connection
/// was established over HTTP/1.1 or HTTP/2.
///
/// # Intended usage
///
/// This type is designed for **integration and end-to-end tests**, where:
/// - panicking on unexpected frames is acceptable,
/// - error handling should be concise,
/// - test code should focus on behavior, not protocol details.
///
/// # Notes
///
/// - All methods assume text-based messaging.
/// - Binary frames and control frames are not currently supported.
/// - Unexpected messages will cause the test to panic.
pub struct TestWebSocket {
    inner: InnerWebSocket,
}

impl Debug for TestWebSocket {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TestWebSocket(..)").finish()
    }
}

impl TestWebSocket {
    /// Creates a new [`TestWebSocket`] from an internal WebSocket representation.
    ///
    /// This constructor is crate-private and intended to be used by
    /// `TestServer` or protocol-specific helpers.
    fn new(inner: InnerWebSocket) -> Self {
        Self { inner }
    }

    /// Creates a [`TestWebSocket`] from an HTTP/1.1 WebSocket connection.
    pub(crate) fn from_http1(ws: WebSocketStream<MaybeTlsStream<TcpStream>>) -> Self {
        Self::new(InnerWebSocket::Http1(ws))
    }

    /// Creates a [`TestWebSocket`] from an HTTP/2 WebSocket connection.
    pub(crate) fn from_http2(ws: WebSocketStream<TokioIo<Upgraded>>) -> Self {
        Self::new(InnerWebSocket::Http2(ws))
    }

    /// Sends a text message over the WebSocket connection.
    ///
    /// # Panics
    ///
    /// Panics if the message cannot be sent or the connection is closed.
    ///
    /// This behavior is intentional and suitable for test environments,
    /// where such failures should immediately fail the test.
    pub async fn send_text(&mut self, text: &str) {
        let msg = Message::Text(text.into());

        match &mut self.inner {
            InnerWebSocket::Http1(ws) => ws
                .send(msg)
                .await
                .unwrap(),
            InnerWebSocket::Http2(ws) => ws
                .send(msg)
                .await
                .unwrap(),
        }
    }

    /// Receives the next text message from the WebSocket connection.
    ///
    /// # Returns
    ///
    /// The received text payload.
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - the connection is closed,
    /// - an error occurs while receiving,
    /// - the next frame is not a text message.
    ///
    /// This strict behavior helps keep test assertions simple and explicit.
    pub async fn recv_text(&mut self) -> String {
        match &mut self.inner {
            InnerWebSocket::Http1(ws) => match ws.next().await {
                Some(Ok(Message::Text(t))) => t.to_string(),
                other => panic!("Unexpected message: {:?}", other),
            },
            InnerWebSocket::Http2(ws) => match ws.next().await {
                Some(Ok(Message::Text(t))) => t.to_string(),
                other => panic!("Unexpected message: {:?}", other),
            },
        }
    }

    /// Gracefully closes the WebSocket connection.
    ///
    /// Any errors during close are ignored, as this method is intended
    /// for cleanup at the end of tests.
    pub async fn close(self) {
        let _ = match self.inner { 
            InnerWebSocket::Http1(mut ws) => ws.close(None).await,
            InnerWebSocket::Http2(mut ws) => ws.close(None).await,
        };
    }
}
