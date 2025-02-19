//! Type extractors and converters for WebSockets

use crate::error::Error;
use std::borrow::Cow;
use tokio_tungstenite::tungstenite::Message;

/// A trait for types that can be returned from WebSocket handler and converted to [`Message`]
pub trait IntoMessage {
    fn into_message(self) -> Message;
}

/// A trait for types that that can be inferred from WebSocket [`Message`]
pub trait FromMessage: Sized {
    fn from_message(msg: Message) -> Result<Self, Error>;
}

impl IntoMessage for Message {
    #[inline]
    fn into_message(self) -> Message {
        self
    }
}

impl FromMessage for Message {
    #[inline]
    fn from_message(msg: Message) -> Result<Self, Error> {
        Ok(msg)
    }
}

impl IntoMessage for &'static str {
    #[inline]
    fn into_message(self) -> Message {
        Message::text(self)
    }
}

impl IntoMessage for String {
    #[inline]
    fn into_message(self) -> Message {
        Message::text(self)
    }
}

impl FromMessage for String {
    #[inline]
    fn from_message(msg: Message) -> Result<Self, Error> {
        let utf_bytes = msg
            .into_text()
            .map_err(Error::from)?;
        Ok(utf_bytes.as_str().into())
    }
}

impl IntoMessage for Box<str> {
    #[inline]
    fn into_message(self) -> Message {
        Message::text(&*self)
    }
}

impl FromMessage for Box<str> {
    #[inline]
    fn from_message(msg: Message) -> Result<Self, Error> {
        String::from_message(msg).map(|s| s.into_boxed_str())
    }
}

impl IntoMessage for Box<[u8]> {
    #[inline]
    fn into_message(self) -> Message {
        Message::binary(self)
    }
}

impl FromMessage for Box<[u8]> {
    #[inline]
    fn from_message(msg: Message) -> Result<Self, Error> {
        Ok(msg.into_data().to_vec().into_boxed_slice())
    }
}

impl IntoMessage for Cow<'static, str> {
    #[inline]
    fn into_message(self) -> Message {
        Message::text(self.into_owned())
    }
}

impl FromMessage for Cow<'static, str> {
    #[inline]
    fn from_message(msg: Message) -> Result<Self, Error> {
        let utf_bytes = msg
            .into_text()
            .map_err(Error::from)?;
        Ok(Cow::Owned(utf_bytes.as_str().into()))
    }
}
