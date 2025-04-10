//! Utilities for SSE (Server-Sent Events)

use bytes::{BufMut, Bytes, BytesMut};
use serde::Serialize;

/// Represents a single SSE event message
#[derive(Debug, Default, Clone)]
pub struct Event {
    buffer: BytesMut,
}

impl Event {
    pub fn new(name: &str, value: impl AsRef<[u8]>) -> Self {
        let mut buffer = BytesMut::new();
        
        buffer.extend_from_slice(name.as_bytes());
        buffer.put_u8(b':');
        buffer.put_u8(b' ');
        buffer.extend_from_slice(value.as_ref());
        buffer.put_u8(b'\n');
        
        Self { buffer }
    }
}

impl<T: Serialize> From<T> for Event {
    #[inline]
    fn from(value: T) -> Self {
        match serde_json::to_vec(&value) { 
            Ok(v) => Self::new("data", v),
            Err(err) => Self::new("error", err.to_string()),
        }
    }
}

impl From<Event> for Bytes {
    #[inline]
    fn from(e: Event) -> Self {
        let mut buffer = e.buffer;
        buffer.put_u8(b'\n');
        buffer.freeze()
    }
}

#[cfg(test)]
mod tests {
    use bytes::Bytes;
    use serde::Serialize;
    use super::Event;
    
    #[test]
    fn it_creates_string_event() {
        let event = Event::new("data", "hi!");
        
        let bytes: Bytes = event.into();

        assert_eq!(String::from_utf8_lossy(&bytes), "data: hi!\n\n");
    }

    #[test]
    fn it_creates_json_event() {
        let data = Test { value: "test".into() };
        let event: Event = data.into();

        let bytes: Bytes = event.into();

        assert_eq!(String::from_utf8_lossy(&bytes), "data: {\"value\":\"test\"}\n\n");
    }

    #[derive(Serialize)]
    struct Test {
        value: String,
    }
}