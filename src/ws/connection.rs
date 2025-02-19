use super::{WebSocket, WebSocketError};
use hyper_util::rt::TokioIo;
use std::future::Future;
use futures_util::future::{ready, Ready};
use sha1::{Digest, Sha1};
use base64::{engine::general_purpose::STANDARD, Engine as _};

use hyper::{
    http::{request::Parts, Uri, Version},
    upgrade::OnUpgrade
};

use crate::{
    HttpResult, ok, status,
    http::endpoints::args::{FromPayload, Payload, Source},
    error::{Error, handler::{WeakErrorHandler, call_weak_err_handler}}, 
    headers::{
        HeaderValue, 
        CONNECTION, 
        SEC_WEBSOCKET_ACCEPT,
        SEC_WEBSOCKET_KEY,
        SEC_WEBSOCKET_PROTOCOL,
        SEC_WEBSOCKET_VERSION,
        UPGRADE
    } 
};

use tokio_tungstenite::{
    tungstenite::protocol::{Role, WebSocketConfig},
    WebSocketStream,
};

/// Represents the extractor for establishing WebSockets connections
pub struct WebSocketConnection {
    uri: Uri,
    config: WebSocketConfig,
    error_handler: WeakErrorHandler,
    on_upgrade: OnUpgrade,
    protocol: Option<HeaderValue>,
    sec_websocket_key: Option<HeaderValue>,
    sec_websocket_protocol: Option<HeaderValue>,
}

impl WebSocketConnection {
    /// Sets the read buffer capacity. 
    /// 
    /// Default: 128KiB
    pub fn with_read_buffer_size(mut self, size: usize) -> Self {
        self.config.read_buffer_size = size;
        self
    }

    /// Sets the target minimum size of the write buffer to reach before writing the data
    /// to the underlying stream.
    ///
    /// Default: 128 KiB.
    ///
    /// If set to `0` each message will be eagerly written to the underlying stream.
    /// It is often more optimal to allow them to buffer a little, hence the default value.
    ///
    /// Note: [`flush`](SinkExt::flush) will always fully write the buffer regardless.
    pub fn with_write_buffer_size(mut self, size: usize) -> Self {
        self.config.write_buffer_size = size;
        self
    }

    /// Sets the max size of the write buffer in bytes. Setting this can provide backpressure
    /// in the case the write buffer is filling up due to write errors.
    ///
    /// Default: not set/unlimited
    ///
    /// Note: The write buffer only builds up past [`write_buffer_size`](Self::write_buffer_size)
    /// when writes to the underlying stream are failing. So the **write buffer can not
    /// fill up if you are not observing write errors even if not flushing**.
    ///
    /// Note: Should always be at least [`write_buffer_size + 1 message`](Self::write_buffer_size)
    /// and probably a little more depending on error handling strategy.
    pub fn with_max_write_buffer_size(mut self, max: usize) -> Self {
        self.config.max_write_buffer_size = max;
        self
    }

    /// Sets the maximum message size
    /// 
    /// Default: 64 MiB
    pub fn with_max_message_size(mut self, max: usize) -> Self {
        self.config.max_message_size = Some(max);
        self
    }

    /// Sets the maximum frame size
    /// 
    /// Default: 16 MiB
    pub fn with_max_frame_size(mut self, max: usize) -> Self {
        self.config.max_frame_size = Some(max);
        self
    }

    /// Sets/unsets a web-server to accept unmasked frames
    /// 
    /// Default: `false`
    pub fn with_accept_unmasked_frames(mut self, accept: bool) -> Self {
        self.config.accept_unmasked_frames = accept;
        self
    }

    /// Sets the protocols known by server.
    /// 
    /// Default: empty list
    pub fn with_protocols<const N: usize>(mut self, known: [&'static str; N]) -> Self {
        if let Some(sec_websocket_protocol) = self
            .sec_websocket_protocol
            .as_ref()
            .and_then(|p| p.to_str().ok())
        {
            let mut split = sec_websocket_protocol
                .split(',')
                .map(str::trim);
            self.protocol = known
                .iter()
                .find(|&&proto| split.any(|req_proto| req_proto == proto))
                .map(|&protocol| HeaderValue::from_static(protocol));
        }
        self
    }
    
    /// Upgrades a connection and call a mapped `handler` with the stream.
    pub fn on<F, Fut>(self, func: F) -> HttpResult
    where
        F: FnOnce(WebSocket) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let WebSocketConnection {
            uri,
            config,
            protocol,
            on_upgrade,
            error_handler,
            sec_websocket_key,
            sec_websocket_protocol
        } = self;

        tokio::spawn(async move {
            let upgraded = match on_upgrade.await {
                Ok(upgraded) => TokioIo::new(upgraded),
                Err(err) => {
                    _ = call_weak_err_handler(
                        error_handler, &uri,
                        Error::server_error(err)).await;
                    return;
                }
            };

            let stream = WebSocketStream::from_raw_socket(
                upgraded,
                Role::Server,
                Some(config))
                .await;

            let socket = WebSocket::new(stream, protocol);
            func(socket).await;
        });

        let http_response = if let Some(sec_websocket_key) = &sec_websocket_key {
            let accept_key = Self::generate_websocket_accept_key(sec_websocket_key.as_bytes());
            status!(101, [
                (UPGRADE, super::WEBSOCKET),
                (CONNECTION, super::UPGRADE),
                (SEC_WEBSOCKET_ACCEPT, accept_key)
            ])
        } else {
            ok!()
        };

        match (http_response, sec_websocket_protocol) {
            (Ok(response), None) => Ok(response),
            (Err(err), _) => Err(err),
            (Ok(mut response), Some(sec_websocket_protocol)) => {
                response
                    .headers_mut()
                    .insert(SEC_WEBSOCKET_PROTOCOL, sec_websocket_protocol);
                Ok(response)
            }
        }
    }

    #[inline]
    fn generate_websocket_accept_key(key: &[u8]) -> String {
        let mut hasher = Sha1::new();
        hasher.update(key);
        hasher.update(super::WEBSOCKET_GUID.as_bytes());
        STANDARD.encode(hasher.finalize())
    }
}

impl TryFrom<&Parts> for WebSocketConnection {
    type Error = Error;

    #[inline]
    fn try_from(parts: &Parts) -> Result<Self, Self::Error> {
        let sec_websocket_key = if parts.version <= Version::HTTP_11 {
            if matches!(parts.headers.get(&UPGRADE), Some(upgrade) if !upgrade.as_bytes().eq_ignore_ascii_case(super::WEBSOCKET.as_bytes())) {
                return Err(WebSocketError::invalid_upgrade_header()); 
            }

            if matches!(parts.headers.get(&CONNECTION), Some(conn) if !conn.as_bytes().eq_ignore_ascii_case(super::UPGRADE.as_bytes())) {
                return Err(WebSocketError::invalid_connection_header()); 
            }

            if matches!(parts.headers.get(&SEC_WEBSOCKET_VERSION), Some(version) if version != super::VERSION) {
                return Err(WebSocketError::invalid_version_header()); 
            }

            let key = parts.headers
                .get(&SEC_WEBSOCKET_KEY)
                .ok_or(WebSocketError::websocket_key_missing())?
                .clone();
            Some(key)
        } else {
            None
        };
        
        // use remove instead of get
        let on_upgrade = parts.extensions
            .get::<OnUpgrade>()
            .ok_or(WebSocketError::not_upgradable_connection())?
            .clone();
        
        let error_handler = parts.extensions
            .get::<WeakErrorHandler>()
            .ok_or(Error::server_error("Server error: error handler is missing"))?
            .clone();
        
        let sec_websocket_protocol = parts.headers
            .get(&SEC_WEBSOCKET_PROTOCOL)
            .cloned();

        Ok(Self {
            uri: parts.uri.clone(),
            config: Default::default(),
            protocol: None,
            on_upgrade,
            error_handler,
            sec_websocket_key,
            sec_websocket_protocol,
        })
    }
}

impl FromPayload for WebSocketConnection {
    type Future = Ready<Result<Self, Error>>;

    #[inline]
    fn from_payload(payload: Payload) -> Self::Future {
        let Payload::Parts(parts) = payload else { unreachable!() };
        ready(parts.try_into())
    }

    #[inline]
    fn source() -> Source {
        Source::Parts
    }
}
