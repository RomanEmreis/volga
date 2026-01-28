//! Type extractors and converters for WebSockets

use crate::error::Error;
use crate::ws::WebSocket;
use bytes::Bytes;
use tokio_tungstenite::tungstenite;
use std::{
    borrow::Cow, 
    fmt, 
    future::Future, 
    ops::{Deref, DerefMut}
};

/// Represents various forms of WebSockets message
/// 
/// See also [`tungstenite::Message`]
#[derive(Debug)]
pub struct Message(pub(super) tungstenite::Message);

impl Message {
    /// Unwwraps the inner message
    #[inline]
    pub fn into_inner(self) -> tungstenite::Message {
        self.0
    }
}

impl Deref for Message {
    type Target = tungstenite::Message;
    
    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl DerefMut for Message {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl fmt::Display for Message {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.0.fmt(f)
    }
}

impl From<tungstenite::Message> for Message {
    #[inline]
    fn from(msg: tungstenite::Message) -> Self {
        Message(msg)
    }
}

impl From<Message> for tungstenite::Message {
    #[inline]
    fn from(msg: Message) -> Self {
        msg.into_inner()
    }
}

impl TryFrom<&str> for Message {
    type Error = Error;
    
    #[inline]
    fn try_from(str: &str) -> Result<Self, Self::Error> {
        Ok(Self(str.into()))
    }
}

impl TryFrom<String> for Message {
    type Error = Error;
    
    #[inline]
    fn try_from(str: String) -> Result<Self, Self::Error> {
        Ok(Self(str.into()))
    }
}

impl TryFrom<Message> for String {
    type Error = Error;

    #[inline]
    fn try_from(msg: Message) -> Result<Self, Self::Error> {
        let utf_bytes = msg.0
            .into_text()
            .map_err(Error::from)?;
        Ok(utf_bytes.as_str().into())
    }
}

impl TryFrom<Box<str>> for Message {
    type Error = Error;

    #[inline]
    fn try_from(str: Box<str>) -> Result<Self, Self::Error> {
        Ok(Self(tungstenite::Message::text(&*str)))
    }
}

impl TryFrom<Message> for Box<str> {
    type Error = Error;

    #[inline]
    fn try_from(msg: Message) -> Result<Self, Self::Error> {
        String::try_from(msg)
            .map(|s| s.into_boxed_str())
    }
}

impl TryFrom<&[u8]> for Message {
    type Error = Error;

    #[inline]
    fn try_from(slice: &[u8]) -> Result<Self, Self::Error> {
        Ok(Self(slice.into()))
    }
}

impl TryFrom<Vec<u8>> for Message {
    type Error = Error;

    #[inline]
    fn try_from(vec: Vec<u8>) -> Result<Self, Self::Error> {
        Ok(Self(vec.into()))
    }
}

impl TryFrom<Message> for Vec<u8> {
    type Error = Error;

    #[inline]
    fn try_from(msg: Message) -> Result<Self, Self::Error> {
        Ok(msg.0.into_data().to_vec())
    }
}

impl TryFrom<Box<[u8]>> for Message {
    type Error = Error;

    #[inline]
    fn try_from(vec: Box<[u8]>) -> Result<Self, Self::Error> {
        Ok(Self(tungstenite::Message::binary(vec)))
    }
}

impl TryFrom<Message> for Box<[u8]> {
    type Error = Error;

    #[inline]
    fn try_from(msg: Message) -> Result<Self, Self::Error> {
        Ok(msg.0.into_data()
            .to_vec()
            .into_boxed_slice())
    }
}

impl TryFrom<Cow<'_, str>> for Message {
    type Error = Error;

    #[inline]
    fn try_from(str: Cow<'_, str>) -> Result<Self, Self::Error> {
        Ok(Self(tungstenite::Message::text(str.into_owned())))
    }
}

impl TryFrom<Message> for Cow<'_, str> {
    type Error = Error;

    #[inline]
    fn try_from(msg: Message) -> Result<Self, Self::Error> {
        let utf_bytes = msg.0
            .into_text()
            .map_err(Error::from)?;
        Ok(Cow::Owned(utf_bytes.as_str().into()))
    }
}

impl TryFrom<Cow<'_, [u8]>> for Message {
    type Error = Error;

    #[inline]
    fn try_from(str: Cow<'_, [u8]>) -> Result<Self, Self::Error> {
        Ok(Self(tungstenite::Message::binary(str.into_owned())))
    }
}

impl TryFrom<Message> for Cow<'_, [u8]> {
    type Error = Error;

    #[inline]
    fn try_from(msg: Message) -> Result<Self, Self::Error> {
        Ok(Cow::Owned(msg.0.into_data().into()))
    }
}

impl TryFrom<Bytes> for Message {
    type Error = Error;

    #[inline]
    fn try_from(bytes: Bytes) -> Result<Self, Self::Error> {
        Ok(Self(tungstenite::Message::binary(bytes)))
    }
}

impl TryFrom<Message> for Bytes {
    type Error = Error;

    #[inline]
    fn try_from(msg: Message) -> Result<Self, Self::Error> {
        Ok(msg.0.into_data())
    }
}

/// Describes a generic WebSocket/WebTransport handler that could take a [`WebSocket`] 
/// and 0 or N parameters of types
pub trait WebSocketHandler<Args>: Clone + Send + Sync + 'static {
    /// The type of valure returned from a WebSocket/WebTransport handler
    type Output;
    /// Output future of a WebSocket/WebTransport handler
    type Future: Future<Output = Self::Output> + Send;

    /// Calls a WebSocket/WebTransport handler
    fn call(&self, ws: WebSocket, args: Args) -> Self::Future;
}

/// Describes a generic WebSocket/WebTransport message handler that could take a message 
/// in a format that implements the[`FromMessage`] and 0 or N parameters of types
pub trait MessageHandler<M: TryFrom<Message>, Args>: Clone + Send + Sync + 'static {
    /// The type of valure returned from a WebSocket/WebTransport message handler
    type Output;
    /// Output future of a WebSocket/WebTransport message handler
    type Future: Future<Output = Self::Output> + Send;

    /// Calls a WebSocket/WebTransport message handler
    fn call(&self, msg: M, args: Args) -> Self::Future;
}

macro_rules! define_generic_ws_handler ({ $($param:ident)* } => {
    impl<Func, Fut: Send, $($param,)*> WebSocketHandler<($($param,)*)> for Func
    where
        Func: Fn(WebSocket, $($param),*) -> Fut + Send + Sync + Clone + 'static,
        Fut: Future,
    {
        type Output = Fut::Output;
        type Future = Fut;

        #[inline]
        #[allow(non_snake_case)]
        fn call(&self, ws: WebSocket, ($($param,)*): ($($param,)*)) -> Self::Future {
            (self)(ws, $($param,)*)
        }
    }
});

macro_rules! define_generic_message_handler ({ $($param:ident)* } => {
    impl<M, Func, Fut: Send, $($param,)*> MessageHandler<M, ($($param,)*)> for Func
    where
        Func: Fn(M, $($param),*) -> Fut + Send + Sync + Clone + 'static,
        M: TryFrom<Message> + Send,
        Fut: Future + 'static,
    {
        type Output = Fut::Output;
        type Future = Fut;

        #[inline]
        #[allow(non_snake_case)]
        fn call(&self, msg: M, ($($param,)*): ($($param,)*)) -> Self::Future {
            (self)(msg, $($param,)*)
        }
    }
});

define_generic_ws_handler! {}
define_generic_ws_handler! { T1 }
define_generic_ws_handler! { T1 T2 }
define_generic_ws_handler! { T1 T2 T3 }
define_generic_ws_handler! { T1 T2 T3 T4 }
define_generic_ws_handler! { T1 T2 T3 T4 T5 }

define_generic_message_handler! {}
define_generic_message_handler! { T1 }
define_generic_message_handler! { T1 T2 }
define_generic_message_handler! { T1 T2 T3 }
define_generic_message_handler! { T1 T2 T3 T4 }
define_generic_message_handler! { T1 T2 T3 T4 T5 }

#[cfg(test)]
mod tests {
    use super::{Message, MessageHandler};
    use bytes::Bytes;
    use std::borrow::Cow;
    use tokio_tungstenite::tungstenite;

    #[test]
    fn it_handles_string_messages() {
        let expected = String::from("test");

        let message: Message = expected.clone().try_into().unwrap();
        let string = String::try_from(message).unwrap();

        assert_eq!(string, expected);
    }

    #[test]
    fn it_handles_boxed_string_messages() {
        let expected = String::from("test").into_boxed_str();

        let message: Message = expected.clone().try_into().unwrap();
        let string = Box::try_from(message).unwrap();

        assert_eq!(string, expected);
    }

    #[test]
    fn it_handles_str_messages() {
        let expected = "test";

        let message: Message = expected.try_into().unwrap();
        let string = String::try_from(message).unwrap();

        assert_eq!(string, expected);
    }

    #[test]
    fn it_handles_bytes_messages() {
        let expected = Bytes::from("test");

        let message: Message = expected.clone().try_into().unwrap();
        let bytes = Bytes::try_from(message).unwrap();

        assert_eq!(bytes, expected);
    }

    #[test]
    fn it_handles_vec_messages() {
        let expected = vec![1,2,3];

        let message: Message = expected.clone().try_into().unwrap();
        let vec = Vec::try_from(message).unwrap();

        assert_eq!(vec, expected);
    }

    #[test]
    fn it_handles_boxed_slice_messages() {
        let expected = vec![1,2,3].into_boxed_slice();

        let message: Message = expected.clone().try_into().unwrap();
        let vec = Box::try_from(message).unwrap();

        assert_eq!(vec, expected);
    }

    #[test]
    fn it_handles_slice_messages() {
        let expected = [1,2,3];

        let message: Message = expected.as_ref().try_into().unwrap();
        let string = Vec::try_from(message).unwrap();

        assert_eq!(string, expected);
    }

    #[test]
    fn it_handles_cow_str_messages() {
        let str = String::from("test");
        let expected = Cow::<str>::Owned(str);

        let message: Message = expected.clone().try_into().unwrap();
        let vec = Cow::<str>::try_from(message).unwrap();

        assert_eq!(vec, expected);
    }

    #[test]
    fn it_handles_cow_slice_messages() {
        let vec = vec![1,2,3];
        let expected = Cow::<[u8]>::Owned(vec);

        let message: Message = expected.clone().try_into().unwrap();
        let vec = Cow::<[u8]>::try_from(message).unwrap();

        assert_eq!(vec, expected);
    }

    #[test]
    fn it_formats_message_display() {
        let message = Message(tungstenite::Message::text("hello"));
        assert_eq!(message.to_string(), "hello");
    }

    #[tokio::test]
    async fn message_handler_invokes_function_with_args() {
        let handler = |msg: String, tag: &'static str| async move {
            format!("{tag}:{msg}")
        };
        let message: Message = "ping".try_into().unwrap();
        let output = MessageHandler::call(&handler, String::try_from(message).unwrap(), ("ws",)).await;

        assert_eq!(output, "ws:ping");
    }

}
