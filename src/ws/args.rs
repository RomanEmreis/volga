﻿//! Type extractors and converters for WebSockets

use crate::error::Error;
use bytes::Bytes;
use std::borrow::Cow;
use tokio_tungstenite::tungstenite::Message;

/// A trait for types that can be returned from WebSocket handler and converted to [`Message`]
pub trait IntoMessage {
    fn into_message(self) -> Result<Message, Error>;
}

/// A trait for types that that can be inferred from WebSocket [`Message`]
pub trait FromMessage: Sized {
    fn from_message(msg: Message) -> Result<Self, Error>;
}

impl IntoMessage for Message {
    #[inline]
    fn into_message(self) -> Result<Message, Error> {
        Ok(self)
    }
}

impl FromMessage for Message {
    #[inline]
    fn from_message(msg: Message) -> Result<Self, Error> {
        Ok(msg)
    }
}

impl IntoMessage for &str {
    #[inline]
    fn into_message(self) -> Result<Message, Error> {
        Ok(self.into())
    }
}

impl IntoMessage for String {
    #[inline]
    fn into_message(self) -> Result<Message, Error> {
        Ok(self.into())
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
    fn into_message(self) -> Result<Message, Error> {
        Ok(Message::text(&*self))
    }
}

impl FromMessage for Box<str> {
    #[inline]
    fn from_message(msg: Message) -> Result<Self, Error> {
        String::from_message(msg)
            .map(|s| s.into_boxed_str())
    }
}

impl IntoMessage for &[u8] {
    #[inline]
    fn into_message(self) -> Result<Message, Error> {
        Ok(self.into())
    }
}

impl IntoMessage for Vec<u8> {
    #[inline]
    fn into_message(self) -> Result<Message, Error> {
        Ok(self.into())
    }
}

impl FromMessage for Vec<u8> {
    #[inline]
    fn from_message(msg: Message) -> Result<Self, Error> {
        Ok(msg.into_data().to_vec())
    }
}

impl IntoMessage for Box<[u8]> {
    #[inline]
    fn into_message(self) -> Result<Message, Error> {
        Ok(Message::binary(self))
    }
}

impl FromMessage for Box<[u8]> {
    #[inline]
    fn from_message(msg: Message) -> Result<Self, Error> {
        Ok(msg.into_data()
            .to_vec()
            .into_boxed_slice())
    }
}

impl IntoMessage for Cow<'_, str> {
    #[inline]
    fn into_message(self) -> Result<Message, Error> {
        Ok(Message::text(self.into_owned()))
    }
}

impl FromMessage for Cow<'_, str> {
    #[inline]
    fn from_message(msg: Message) -> Result<Self, Error> {
        let utf_bytes = msg
            .into_text()
            .map_err(Error::from)?;
        Ok(Cow::Owned(utf_bytes.as_str().into()))
    }
}

impl IntoMessage for Cow<'_, [u8]> {
    #[inline]
    fn into_message(self) -> Result<Message, Error> {
        Ok(Message::binary(self.into_owned()))
    }
}

impl FromMessage for Cow<'_, [u8]> {
    #[inline]
    fn from_message(msg: Message) -> Result<Self, Error> {
        Ok(Cow::Owned(msg.into_data().into()))
    }
}

impl FromMessage for Bytes {
    #[inline]
    fn from_message(msg: Message) -> Result<Self, Error> {
        Ok(msg.into_data())
    }
}

impl IntoMessage for Bytes {
    #[inline]
    fn into_message(self) -> Result<Message, Error> {
        Ok(Message::binary(self))
    }
}

#[cfg(test)]
mod tests {
    use super::{FromMessage, IntoMessage};
    use bytes::Bytes;
    use std::borrow::Cow;

    #[test]
    fn it_handles_string_messages() {
        let expected = String::from("test");

        let message = expected.clone().into_message().unwrap();
        let string = String::from_message(message).unwrap();

        assert_eq!(string, expected);
    }

    #[test]
    fn it_handles_boxed_string_messages() {
        let expected = String::from("test").into_boxed_str();

        let message = expected.clone().into_message().unwrap();
        let string = Box::from_message(message).unwrap();

        assert_eq!(string, expected);
    }

    #[test]
    fn it_handles_str_messages() {
        let expected = "test";

        let message = expected.into_message().unwrap();
        let string = String::from_message(message).unwrap();

        assert_eq!(string, expected);
    }

    #[test]
    fn it_handles_bytes_messages() {
        let expected = Bytes::from("test");

        let message = expected.clone().into_message().unwrap();
        let bytes = Bytes::from_message(message).unwrap();

        assert_eq!(bytes, expected);
    }

    #[test]
    fn it_handles_vec_messages() {
        let expected = vec![1,2,3];

        let message = expected.clone().into_message().unwrap();
        let vec = Vec::from_message(message).unwrap();

        assert_eq!(vec, expected);
    }

    #[test]
    fn it_handles_boxed_slice_messages() {
        let expected = vec![1,2,3].into_boxed_slice();

        let message = expected.clone().into_message().unwrap();
        let vec = Box::from_message(message).unwrap();

        assert_eq!(vec, expected);
    }

    #[test]
    fn it_handles_slice_messages() {
        let expected = [1,2,3];

        let message = expected.into_message().unwrap();
        let string = Vec::from_message(message).unwrap();

        assert_eq!(string, expected);
    }

    #[test]
    fn it_handles_cow_str_messages() {
        let str = String::from("test");
        let expected = Cow::<str>::Owned(str);

        let message = expected.clone().into_message().unwrap();
        let vec = Cow::<str>::from_message(message).unwrap();

        assert_eq!(vec, expected);
    }

    #[test]
    fn it_handles_cow_slice_messages() {
        let vec = vec![1,2,3];
        let expected = Cow::<[u8]>::Owned(vec);

        let message = expected.clone().into_message().unwrap();
        let vec = Cow::<[u8]>::from_message(message).unwrap();

        assert_eq!(vec, expected);
    }
}
