use crate::{error::Error, headers::HeaderValue};
use super::{FromMessage, IntoMessage};

use futures_util::{  
    sink::{Sink, SinkExt},
    stream::{
        Stream,
        StreamExt, 
        SplitSink, 
        SplitStream
    }
};

use hyper_util::rt::TokioIo;
use hyper::upgrade::Upgraded;

use std::{
    future::Future, 
    pin::Pin, 
    task::{ready, Context, Poll}
};

use tokio_tungstenite::{
    tungstenite::Message,
    WebSocketStream,
};

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
    pub async fn recv<T: FromMessage>(&mut self) -> Option<Result<T, Error>> {
        self.next()
            .await
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

    /// Returns the selected WebSocket sub-protocol, if there is any.
    pub fn protocol(&self) -> Option<&HeaderValue> {
        self.protocol.as_ref()
    }
    
    /// Splits this `Stream + Sink` object into separate `Sink` and `Stream` objects.
    /// This can be useful when you want to split ownership between tasks, 
    /// or allow direct interaction between the two objects (e.g. via `Sink::send_all`).
    #[inline]
    pub fn split(self) -> (
        SplitSink<WebSocketStream<TokioIo<Upgraded>>, Message>,
        SplitStream<WebSocketStream<TokioIo<Upgraded>>>
    ) 
    {
        self.inner.split()
    }

    /// Maps a `handler` that has to be called every time a message is received.
    #[inline]
    pub async fn on_msg<F, M, R, Fut>(&mut self, handler: F)
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
                None => return Poll::Ready(None),
                Some(Err(err)) => return Poll::Ready(Some(Err(err.into()))),
                Some(Ok(msg)) => {
                    let Message::Frame(_) = msg else { return Poll::Ready(Some(Ok(msg))) };
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
        match Pin::new(&mut self.inner).start_send(item) {
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