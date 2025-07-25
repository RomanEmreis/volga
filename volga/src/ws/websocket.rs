use crate::{error::Error, headers::HeaderValue};
use super::Message;

use futures_util::{sink::{Sink, SinkExt}, stream::{
    Stream,
    StreamExt,
    SplitSink,
    SplitStream
}};

use hyper_util::rt::TokioIo;
use hyper::upgrade::Upgraded;

use std::{
    future::Future, 
    pin::Pin, 
    task::{ready, Context, Poll}
};

use tokio_tungstenite::{tungstenite, WebSocketStream};

/// A [`Sink`] part of [`WebSocket`] split 
pub struct WsSink(SplitSink<WebSocketStream<TokioIo<Upgraded>>, tungstenite::Message>);

/// A [`Stream`] part of [`WebSocket`] split
pub struct WsStream(SplitStream<WebSocketStream<TokioIo<Upgraded>>>);

impl WsSink {
    /// Unwraps the inner [`Sink`]
    #[inline]
    pub fn into_inner(self) -> SplitSink<WebSocketStream<TokioIo<Upgraded>>, tungstenite::Message> {
        self.0
    }
    
    /// Sends a message.
    #[inline]
    pub async fn send<T: TryInto<Message, Error = Error>>(&mut self, msg: T) -> Result<(), Error> {
        let msg = msg.try_into()?.into();
        self.0.send(msg)
            .await
            .map_err(Error::from)
    }
}

impl WsStream {
    /// Unwraps the inner [`Stream`]
    #[inline]
    pub fn into_inner(self) -> SplitStream<WebSocketStream<TokioIo<Upgraded>>> {
        self.0
    }
    
    /// Receives a message.
    #[inline]
    pub async fn recv<T: TryFrom<Message, Error = Error>>(&mut self) -> Option<Result<T, Error>> {
        self.0.next()
            .await
            .map(|result| result
                .map_err(Error::from)
                .and_then(|msg| T::try_from(Message(msg))))
    }    
}

/// Represents a stream of WebSocket messages.
pub struct WebSocket {
    inner: WebSocketStream<TokioIo<Upgraded>>,
    protocol: Option<HeaderValue>,
}

impl WebSocket {
    /// Creates a new [`WebSocket`]
    #[inline]
    pub(super) fn new(
        inner: WebSocketStream<TokioIo<Upgraded>>,
        protocol: Option<HeaderValue>
    ) -> Self {
        Self { inner, protocol }
    }

    /// Receives a message.
    #[inline]
    pub async fn recv<T: TryFrom<Message, Error = Error>>(&mut self) -> Option<Result<T, Error>> {
        self.next()
            .await
            .map(|r| r.and_then(|msg| T::try_from(msg)))
    }

    /// Sends a message.
    #[inline]
    pub async fn send<T: TryInto<Message, Error = Error>>(&mut self, msg: T) -> Result<(), Error> {
        let msg = msg.try_into()?;
        self.inner
            .send(msg.into_inner())
            .await
            .map_err(Error::from)
    }

    /// Returns the selected WebSocket sub-protocol, if there is any.
    pub fn protocol(&self) -> Option<&HeaderValue> {
        self.protocol.as_ref()
    }
    
    /// Splits this `Stream + Sink` object into separate `Sink` and `Stream` objects.
    /// This can be useful when you want to split ownership between tasks, 
    /// or allow direct interaction between the two objects (e.g. via `Sink::send_all`).
    #[inline]
    pub fn split(self) -> (WsSink, WsStream) {
        let (tx, rx) = self.inner.split();
        (WsSink(tx), WsStream(rx))
    }

    /// Maps a `handler` that has to be called every time a message is received.
    #[inline]
    pub async fn on_msg<F, M, R, Fut>(&mut self, handler: F)
    where
        F: Fn(M) -> Fut + Send + 'static,
        M: TryFrom<Message, Error = Error>,
        R: TryInto<Message, Error = Error>,
        Fut: Future<Output = R> + Send
    {
        while let Some(msg) = self.recv::<M>().await {
            let msg = match msg { 
                Ok(msg) => msg, 
                Err(_e) => {
                    #[cfg(feature = "tracing")]
                    tracing::error!("Error receiving message: {_e}");
                    return;
                }
            };

            let response = handler(msg).await;
            if let Err(_e) = self.send(response).await {
                #[cfg(feature = "tracing")]
                tracing::error!("Error sending message: {_e}");
                return;
            }
        }
    }
}

impl Stream for WebSocket {
    type Item = Result<Message, Error>;

    #[inline]
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        loop {
            match ready!(self.inner.poll_next_unpin(cx)) {
                None => return Poll::Ready(None),
                Some(Err(err)) => return Poll::Ready(Some(Err(err.into()))),
                Some(Ok(msg)) => {
                    let tungstenite::Message::Frame(_) = msg else { return Poll::Ready(Some(Ok(Message(msg)))) };
                }
            }
        }
    }
}

impl Sink<Message> for WebSocket {
    type Error = Error;

    #[inline]
    fn poll_ready(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match ready!(Pin::new(&mut self.inner).poll_ready(cx)) {
            Ok(()) => Poll::Ready(Ok(())),
            Err(e) => Poll::Ready(Err(Error::server_error(e))),
        }
    }

    #[inline]
    fn start_send(mut self: Pin<&mut Self>, item: Message) -> Result<(), Self::Error> {
        match Pin::new(&mut self.inner).start_send(item.0) {
            Ok(_) => Ok(()),
            Err(err) => Err(Error::server_error(err))
        }
    }

    #[inline]
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match ready!(Pin::new(&mut self.inner).poll_flush(cx)) {
            Ok(_) => Poll::Ready(Ok(())),
            Err(err) => Poll::Ready(Err(Error::server_error(err))),
        }
    }

    #[inline]
    fn poll_close(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        match ready!(Pin::new(&mut self.inner).poll_close(cx)) {
            Ok(_) => Poll::Ready(Ok(())),
            Err(err) => Poll::Ready(Err(Error::server_error(err)))
        }
    }
}
