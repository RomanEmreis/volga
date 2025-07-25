//! Utilities for SSE (Server-Sent Events)

use crate::utils::str::memchr_split;
use indexmap::IndexMap;
use std::time::Duration;
use bytes::{BufMut, Bytes, BytesMut};
use serde::Serialize;

const ID: &str = "id";
const EVENT: &str = "event";
const DATA: &str = "data";
const RETRY: &str = "retry";
const ERROR: &str = "error";

/// Represents a single SSE message
/// 
/// # Example
/// ```no_run
/// use volga::http::sse::Message;
/// 
/// let msg = Message::new()
///     .data("Hello, World!");
/// ```
#[derive(Debug, Default, Clone)]
pub struct Message {
    fields: IndexMap<&'static str, Bytes>,
}

impl Message {
    /// Creates a new [`Message`]
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }
    
    /// Creates an empty [`Message`] (":\n")
    /// 
    /// This can be useful as a keep-alive mechanism if messages might not be sent regularly.
    #[inline]
    pub fn empty() -> Self {
        let mut msg = Self::default();
        let empty_msg = ":\n";
        msg.fields
            .insert(empty_msg, Bytes::from_static(empty_msg.as_bytes()));
        msg
    }
    
    /// Specifies a text `data` for a [`Message`]
    ///
    /// # Example
    /// ```no_run
    /// use volga::http::sse::Message;
    ///
    /// let msg = Message::new()
    ///     .data("Hello, World!");
    /// ```
    #[inline]
    pub fn data(mut self, value: impl AsRef<[u8]>) -> Self {
        let mut buffer = BytesMut::new();
        for line in memchr_split(b'\n', value.as_ref()) {
            buffer.extend(Self::field(DATA, line));
        }
        self.fields
            .insert(DATA, buffer.freeze());
        self
    }

    /// Appends a text `data` to an existing data field in a [`Message`]
    ///
    /// # Example
    /// ```no_run
    /// use volga::http::sse::Message;
    ///
    /// let msg = Message::new()
    ///     .data("Hello, ")
    ///     .append("World!");
    /// ```
    #[inline]
    pub fn append(mut self, value: impl AsRef<[u8]>) -> Self {
        if let Some(data) = self.fields.swap_remove(DATA) {
            let mut data = BytesMut::from(data);
            data.extend(Self::field(DATA, value));
            self.fields
                .insert(DATA, data.freeze());
            self
        } else { 
            self.data(value)
        }
    }

    /// Specifies a JSON `data` for a [`Message`]
    ///
    /// # Example
    /// ```no_run
    /// use volga::http::sse::Message;
    /// use serde::Serialize;
    /// 
    /// #[derive(Serialize)]
    /// struct Event {
    ///     msg: String 
    /// }
    /// 
    /// let event = Event { 
    ///     msg: String::from("Hello, World!")
    /// };
    /// 
    /// let msg = Message::new()
    ///     .json(event);
    /// ```
    #[inline]
    pub fn json<T: Serialize>(self, value: T) -> Self {
        match serde_json::to_vec(&value) {
            Ok(v) => self.data(v),
            Err(err) => self.event(ERROR).data(err.to_string()),
        }
    }

    /// Specifies the `event` field for a [`Message`]
    ///
    /// # Example
    /// ```no_run
    /// use volga::http::sse::Message;
    ///
    /// let msg = Message::new()
    ///     .event("greeting")
    ///     .data("Hello, World!");
    /// ```
    #[inline]
    pub fn event(mut self, name: &str) -> Self {
        self.fields
            .insert(EVENT, Self::field(EVENT, name));
        self
    }

    /// Specifies the event `id` field for a [`Message`]
    ///
    /// # Example
    /// ```no_run
    /// use volga::http::sse::Message;
    ///
    /// let msg = Message::new()
    ///     .id("id")
    ///     .event("greeting")
    ///     .data("Hello, World!");
    /// ``` 
    #[inline]
    pub fn id(mut self, value: impl AsRef<[u8]>) -> Self {
        self.fields
            .insert(ID, Self::field(ID, value));
        self
    }

    /// Specifies the `retry` field for a [`Message`]
    /// 
    /// This represents the reconnection time. If the connection to the server is lost, 
    /// the client will wait for the specified time before attempting to reconnect. 
    ///
    /// # Example
    /// ```no_run
    /// use volga::http::sse::Message;
    /// use std::time::Duration;
    ///
    /// let msg = Message::new()
    ///     .data("Hello, World!")
    ///     .retry(Duration::from_secs(10));
    /// ``` 
    #[inline]
    pub fn retry(mut self, duration: Duration) -> Self {
        let mut buffer = BytesMut::new();

        buffer.extend_from_slice(b"retry:");
        buffer.extend_from_slice(itoa::Buffer::new().format(duration.as_millis()).as_ref());
        buffer.put_u8(b'\n');

        self.fields
            .insert(RETRY,  buffer.freeze());
        self
    }

    /// Adds a new comment for a [`Message`].
    /// 
    /// Multiple calls add multiple comments
    ///
    /// # Example
    /// ```no_run
    /// use volga::http::sse::Message;
    ///
    /// let msg = Message::new()
    ///     .data("Hello, World!")
    ///     .comment("comment 1")
    ///     .comment("comment 2");
    /// ``` 
    #[inline]
    pub fn comment(mut self, value: &'static str) -> Self {
        self.fields
            .insert(
                value, 
                Self::field("", value));
        self
    }

    /// Encodes bytes into SSE message format
    #[inline]
    fn field(name: &str, value: impl AsRef<[u8]>) -> Bytes {
        let mut buffer = BytesMut::new();
        
        buffer.extend_from_slice(name.as_bytes());
        buffer.put_u8(b':');
        buffer.put_u8(b' ');
        buffer.extend_from_slice(value.as_ref());
        buffer.put_u8(b'\n');
        
        buffer.freeze()
    }
}

impl<T: Serialize> From<T> for Message {
    #[inline]
    fn from(value: T) -> Self {
        Self::default().json(value)
    }
}

impl From<Message> for Bytes {
    #[inline]
    fn from(message: Message) -> Self {
        let mut buffer = BytesMut::new();

        for (_, bytes) in message.fields {
            buffer.extend(bytes);
        }
        
        buffer.put_u8(b'\n');
        buffer.freeze()
    }
}



#[cfg(test)]
mod tests {
    use std::time::Duration;
    use bytes::Bytes;
    use serde::Serialize;
    use super::Message;

    #[test]
    fn it_creates_default_message() {
        let event = Message::default();

        let bytes: Bytes = event.into();

        assert_eq!(String::from_utf8_lossy(&bytes), "\n");
    }

    #[test]
    fn it_creates_empty_message() {
        let event = Message::empty();

        let bytes: Bytes = event.into();

        assert_eq!(String::from_utf8_lossy(&bytes), ":\n\n");
    }

    #[test]
    fn it_creates_data_message_with_comment() {
        let event = Message::new()
            .comment("some comment")
            .data("hi!");

        let bytes: Bytes = event.into();

        assert_eq!(String::from_utf8_lossy(&bytes), ": some comment\ndata: hi!\n\n");
    }

    #[test]
    fn it_creates_data_message_with_multiple_comment() {
        let event = Message::new()
            .comment("some comment")
            .data("hi!")
            .comment("another comment")
            .comment("one more comment");

        let bytes: Bytes = event.into();

        assert_eq!(String::from_utf8_lossy(&bytes), ": some comment\ndata: hi!\n: another comment\n: one more comment\n\n");
    }
    
    #[test]
    fn it_creates_string_message() {
        let event = Message::new().data("hi!");
        
        let bytes: Bytes = event.into();

        assert_eq!(String::from_utf8_lossy(&bytes), "data: hi!\n\n");
    }

    #[test]
    fn it_appends_string_data() {
        let event = Message::new()
            .data("Hello")
            .append("World");

        let bytes: Bytes = event.into();

        assert_eq!(String::from_utf8_lossy(&bytes), "data: Hello\ndata: World\n\n");
    }

    #[test]
    fn it_creates_multiline_string_data() {
        let event = Message::new()
            .data("Hello \nbeautiful \nworld!");

        let bytes: Bytes = event.into();

        assert_eq!(String::from_utf8_lossy(&bytes), "data: Hello \ndata: beautiful \ndata: world!\n\n");
    }

    #[test]
    fn it_creates_string_event() {
        let event = Message::new()
            .event("greet")
            .data("hi!");

        let bytes: Bytes = event.into();

        assert_eq!(String::from_utf8_lossy(&bytes), "event: greet\ndata: hi!\n\n");
    }

    #[test]
    fn it_creates_string_event_with_id() {
        let event = Message::new()
            .id("some id")
            .event("greet")
            .data("hi!");

        let bytes: Bytes = event.into();

        assert_eq!(String::from_utf8_lossy(&bytes), "id: some id\nevent: greet\ndata: hi!\n\n");
    }

    #[test]
    fn it_creates_message_with_retry() {
        let event = Message::new()
            .data("hi!")
            .retry(Duration::from_secs(5));

        let bytes: Bytes = event.into();

        assert_eq!(String::from_utf8_lossy(&bytes), "data: hi!\nretry:5000\n\n");
    }

    #[test]
    fn it_creates_json_event() {
        let event = Message::new().json(Test { value: "test".into() });

        let bytes: Bytes = event.into();

        assert_eq!(String::from_utf8_lossy(&bytes), "data: {\"value\":\"test\"}\n\n");
    }

    #[test]
    fn it_converts_json_into_event() {
        let data = Test { value: "test".into() };
        let event: Message = data.into();

        let bytes: Bytes = event.into();

        assert_eq!(String::from_utf8_lossy(&bytes), "data: {\"value\":\"test\"}\n\n");
    }

    #[derive(Serialize)]
    struct Test {
        value: String,
    }
}