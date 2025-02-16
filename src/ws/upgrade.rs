use super::{WebSocket, WebSocketError};
use hyper_util::rt::TokioIo;
use std::future::Future;
use futures_util::future::{ready, Ready};
use sha1::{Digest, Sha1};
use base64::{engine::general_purpose::STANDARD, Engine as _};

use hyper::{
    http::{request::Parts, Method, Version},
    upgrade::OnUpgrade
};

use crate::{
    HttpResult, ok, status,
    http::endpoints::args::{FromPayload, Payload, Source},
    error::Error, headers::{
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
    tungstenite::protocol::{self, WebSocketConfig},
    WebSocketStream,
};

pub struct Upgrade {
    config: WebSocketConfig,
    protocol: Option<HeaderValue>,
    sec_websocket_key: Option<HeaderValue>,
    on_upgrade: OnUpgrade,
    sec_websocket_protocol: Option<HeaderValue>,
}

impl Upgrade {
    pub fn on<F, Fut>(self, func: F) -> HttpResult
    where
        F: FnOnce(WebSocket) -> Fut + Send + 'static,
        Fut: Future<Output = ()> + Send + 'static,
    {
        let Upgrade { 
            config, 
            protocol, 
            sec_websocket_key, 
            on_upgrade, 
            sec_websocket_protocol
        } = self;

        tokio::spawn(async move {
            let upgraded = match on_upgrade.await {
                Ok(upgraded) => TokioIo::new(upgraded),
                Err(_err) => {
                    #[cfg(feature = "tracing")]
                    tracing::error!("{}", _err);
                    return;
                }
            };

            let socket = WebSocketStream::from_raw_socket(
                upgraded, 
                protocol::Role::Server, 
                Some(config)
            ).await;
            func(WebSocket::new(socket, protocol)).await;
        });

        let response = if let Some(sec_websocket_key) = &sec_websocket_key {
            let accept_key = Self::generate_websocket_accept_key(sec_websocket_key.as_bytes());
            status!(101, [
                (UPGRADE, super::WEBSOCKET),
                (CONNECTION, super::UPGRADE),
                (SEC_WEBSOCKET_ACCEPT, accept_key)
            ])
        } else {
            ok!()
        };

        match (response, sec_websocket_protocol) {
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

impl TryFrom<&Parts> for Upgrade {
    type Error = Error;

    #[inline]
    fn try_from(parts: &Parts) -> Result<Self, Self::Error> {
        let sec_key = if parts.version <= Version::HTTP_11 {
            if parts.method != Method::GET {
                return Err(WebSocketError::invalid_method(&Method::GET));  
            }

            if matches!(parts.headers.get(&UPGRADE), Some(upgrade) if !upgrade.as_bytes().eq_ignore_ascii_case(super::WEBSOCKET.as_bytes())) {
                return Err(WebSocketError::invalid_upgrade_header()); 
            }

            if matches!(parts.headers.get(&CONNECTION), Some(conn) if !conn.as_bytes().eq_ignore_ascii_case(super::UPGRADE.as_bytes())) {
                return Err(WebSocketError::invalid_connection_header()); 
            }

            let key = parts.headers
                .get(&SEC_WEBSOCKET_KEY)
                .ok_or(WebSocketError::websocket_key_missing())?
                .clone();
            Some(key)
        } else {
            if parts.method != Method::CONNECT {
                return Err(WebSocketError::invalid_method(&Method::CONNECT));  
            }
            None
        };

        if matches!(parts.headers.get(&SEC_WEBSOCKET_VERSION), Some(version) if version != super::VERSION) {
            return Err(WebSocketError::invalid_version_header()); 
        }

        // use remove instead of get
        let on_upgrade = parts.extensions
            .get::<OnUpgrade>()
            .ok_or(WebSocketError::not_upgradable_connection())?
            .clone();

        let sec_websocket_protocol = parts.headers
            .get(&SEC_WEBSOCKET_PROTOCOL)
            .cloned();

        Ok(Self {
            config: Default::default(),
            protocol: None,
            sec_websocket_key: sec_key,
            on_upgrade,
            sec_websocket_protocol
        })
    }
}

impl FromPayload for Upgrade {
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
