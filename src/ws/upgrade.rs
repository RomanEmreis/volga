use tokio_tungstenite::{
    tungstenite::{
        self as ts,
        protocol::{self, WebSocketConfig},
    },
    WebSocketStream,
};
use crate::headers::HeaderValue;

pub struct Upgrade {
    config: WebSocketConfig,
    protocol: Option<HeaderValue>,
    sec_websocket_key: Option<HeaderValue>,
    on_upgrade: hyper::upgrade::OnUpgrade,
    sec_websocket_protocol: Option<HeaderValue>,
}