//! Utilities for SSE (Server-Sent Events)

use std::fmt::Debug;
use crate::{utils::str::memchr_split};
use futures_util::stream::Stream;
use std::time::Duration;
use std::pin::Pin;
use std::task::{Context, Poll};
use bytes::{BufMut, Bytes, BytesMut};
use serde::Serialize;
use pin_project_lite::pin_project;

const ID: &str = "id";
const EVENT: &str = "event";
const DATA: &str = "data";
const RETRY: &[u8] = b"retry:";
const ERROR: &str = "error";
const NEW_LINE: u8 = b'\n';
const EMPTY: &[u8] = b":\n";

pin_project! {
    /// Wrapper type for SSE streams.
    //#[derive(Clone)]
    pub struct SseStream<S> {
        #[pin]
        inner: S,
    }
}

impl<S> Debug for SseStream<S> {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SseStream(...)").finish()
    }
}

impl<S> SseStream<S> {
    /// Creates a new [`SseStream`] from an inner stream.
    #[inline]
    pub fn new(inner: S) -> Self {
        Self { inner }
    }

    /// Consumes the wrapper and returns the inner stream.
    #[inline]
    pub fn into_inner(self) -> S {
        self.inner
    }
}

impl<S> Stream for SseStream<S>
where
    S: Stream<Item = Bytes>,
{
    type Item = Bytes;

    #[inline]
    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        self.project().inner.poll_next(cx)
    }
}

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
    fields: Vec<SseField>,
}

/// Represents a field kind in an SSE message
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FieldKind {
    Comment,
    Data,
    Event,
    Id,
    Retry,
}

/// Represents a single field in an SSE message
#[derive(Debug, Clone)]
struct SseField {
    kind: FieldKind,
    bytes: Bytes,
}

impl Message {
    /// Creates a new [`Message`]
    #[inline]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a stream that yields this message once.
    #[inline]
    pub fn once(self) -> SseStream<impl Stream<Item = Bytes> + Send + Sync> {
        let bytes: Bytes = self.into();
        SseStream::new(futures_util::stream::iter([bytes]))
    }

    /// Creates a stream that produces this message repeatedly.
    #[inline]
    pub fn repeat(self) -> SseStream<impl Stream<Item = Bytes> + Send + Sync> {
        let bytes: Bytes = self.into();
        SseStream::new(futures_util::stream::repeat(bytes))
    }

    /// Creates an empty [`Message`] (":\n")
    /// 
    /// This can be useful as a keep-alive mechanism if messages might not be sent regularly.
    #[inline]
    pub fn empty() -> Self {
        let mut msg = Self::default();
        msg.fields.push(SseField {
            kind: FieldKind::Comment,
            bytes: Bytes::from_static(EMPTY),
        });
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
        for line in memchr_split(NEW_LINE, value.as_ref()) {
            buffer.extend(Self::field(DATA, line));
        }
        self.remove_fields(FieldKind::Data);
        self.fields.push(SseField {
            kind: FieldKind::Data,
            bytes: buffer.freeze(),
        });
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
        let mut buffer = BytesMut::new();
        for line in memchr_split(NEW_LINE, value.as_ref()) {
            buffer.extend(Self::field(DATA, line));
        }
        self.fields.push(SseField {
            kind: FieldKind::Data,
            bytes: buffer.freeze(),
        });
        self
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
        self.remove_fields(FieldKind::Event);
        self.fields.push(SseField {
            kind: FieldKind::Event,
            bytes: Self::field(EVENT, name),
        });
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
        self.remove_fields(FieldKind::Id);
        self.fields.push(SseField {
            kind: FieldKind::Id,
            bytes: Self::field(ID, value),
        });
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

        buffer.extend_from_slice(RETRY);
        buffer.extend_from_slice(itoa::Buffer::new().format(duration.as_millis()).as_ref());
        buffer.put_u8(NEW_LINE);

        self.remove_fields(FieldKind::Retry);
        self.fields.push(SseField {
            kind: FieldKind::Retry,
            bytes: buffer.freeze(),
        });
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
    pub fn comment(mut self, value: impl AsRef<[u8]>) -> Self {
        self.fields.push(SseField {
            kind: FieldKind::Comment,
            bytes: Self::field("", value),
        });
        self
    }

    /// Removes all fields of a given kind from a [`Message`]
    #[inline]
    fn remove_fields(&mut self, kind: FieldKind) {
        self.fields.retain(|field| field.kind != kind);
    }

    /// Encodes bytes into SSE message format
    #[inline]
    fn field(name: &str, value: impl AsRef<[u8]>) -> Bytes {
        let mut buffer = BytesMut::new();
        
        buffer.extend_from_slice(name.as_bytes());
        buffer.put_u8(b':');
        buffer.put_u8(b' ');
        buffer.extend_from_slice(value.as_ref());
        buffer.put_u8(NEW_LINE);
        
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

        for field in message.fields {
            buffer.extend(field.bytes);
        }
        
        buffer.put_u8(NEW_LINE);
        buffer.freeze()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;
    use bytes::Bytes;
    use futures_util::{pin_mut, StreamExt};
    use serde::Serialize;
    use super::Message;

    #[tokio::test]
    async fn it_creates_message_stream() {
        let stream = Message::new().data("hi!").repeat();
        pin_mut!(stream);
        let bytes = stream.next().await.unwrap();

        assert_eq!(String::from_utf8_lossy(&bytes), "data: hi!\n\n");
    }

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