use crate::{error::Error, headers::HeaderValue};

use futures_util::{  
    sink::SinkExt,
    stream::{Stream, StreamExt}
};

use hyper_util::rt::TokioIo;
use hyper::upgrade::Upgraded;

use std::{
    borrow::Cow, 
    future::Future, 
    pin::Pin, 
    task::{ready, Context, Poll}
};

use tokio_tungstenite::{
    tungstenite::Message,
    WebSocketStream,
};

type WsStream = WebSocketStream<TokioIo<Upgraded>>;

/// Represents a stream of WebSocket messages.
pub struct WebSocket {
    inner: WebSocketStream<TokioIo<Upgraded>>,
    protocol: Option<HeaderValue>,
}

impl WebSocket {
    /// Creates a new WebSocket
    #[inline]
    pub(super) fn new(inner: WsStream, protocol: Option<HeaderValue>) -> Self {
        Self { inner, protocol }
    }

    /// Receives a message.
    #[inline]
    pub async fn recv<T: FromMessage>(&mut self) -> Option<Result<T, Error>> {
        self.next().await
            .map(|r| r.and_then(|msg| T::from_message(msg)))
    }

    /// Sends a message.
    #[inline]
    pub async fn send<T: IntoMessage>(&mut self, msg: T) -> Result<(), Error> {
        self.inner
            .send(msg.into_message())
            .await
            .map_err(Error::from)
    }

    /// Returns the selected WebSocket subprotocol, if there is any.
    pub fn protocol(&self) -> Option<&HeaderValue> {
        self.protocol.as_ref()
    }

    pub async fn on_message<F, M, R, Fut>(&mut self, handler: F)
    where
        F: Fn(M) -> Fut + Send + 'static,
        M: FromMessage,
        R: IntoMessage,
        Fut: Future<Output = R> + Send + 'static
    {
        while let Some(msg) = self.recv::<M>().await {
            let Ok(msg) = msg else { return; };
            let response = handler(msg).await;
            if self.send(response).await.is_err() { return; }
        }
    }
}

impl Stream for WebSocket {
    type Item = Result<Message, Error>;

    #[inline]
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            match ready!(self.inner.poll_next_unpin(cx)) {
                Some(Ok(msg)) => {
                    let Message::Frame(_) = msg else { return Poll::Ready(Some(Ok(msg))) };
                },
                Some(Err(err)) => return Poll::Ready(Some(Err(err.into()))),
                None => return Poll::Ready(None),
            }
        }
    }
}

pub trait IntoMessage {
    fn into_message(self) -> Message;
}

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
        let utf_bytes = msg
            .into_text()
            .map_err(Error::from)?;
        Ok(utf_bytes.as_str().into())
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