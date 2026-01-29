//! WebSocket streaming and messaging utils

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
use tokio_tungstenite::tungstenite::{
    Message as WsMessage,
    Error as WsError,
    protocol::CloseFrame
};

/// A WebSocket connection.
///
/// This is a `Stream + Sink` abstraction over a WebSocket transport. It provides convenient,
/// data-oriented APIs for typical server-side usage.
///
/// - [`WebSocket::recv`] is **data-only**: it yields messages deserialized from text/binary frames
///   and transparently ignores ping/pong. If a close frame is received, it performs a graceful close
///   and ends the stream.
/// - For split ownership between tasks, use [`WebSocket::split`], which yields [`WsSink`] and
///   [`WsStream`]. In split mode, [`WsStream::recv`] yields [`WsEvent`] so close frames can be
///   coordinated with the sink.
#[derive(Debug)]
pub struct WebSocket {
    inner: WebSocketStream<TokioIo<Upgraded>>,
    protocol: Option<HeaderValue>,
}

/// A [`Sink`] half of a split [`WebSocket`].
///
/// This type is produced by [`WebSocket::split`]. It can be moved to a separate task and used
/// to send application messages or to complete the WebSocket close handshake.
///
/// ## Close handshake
/// When the peer requests closing (i.e. the [`WsStream`] yields [`WsEvent::Close`]),
/// you should typically respond by calling [`WsSink::close`] with the received frame.
/// This will send a `Close` control message (echoing the provided frame) and then close the sink.
pub struct WsSink(SplitSink<WebSocketStream<TokioIo<Upgraded>>, WsMessage>);

/// A [`Stream`] half of a split [`WebSocket`].
///
/// This type is produced by [`WebSocket::split`]. It can be moved to a separate task and used
/// to receive messages.
///
/// Unlike [`WebSocket::recv`], which is data-only, [`WsStream::recv`] yields [`WsEvent`] to
/// allow the caller to observe close frames and coordinate the close handshake with [`WsSink`].
pub struct WsStream(SplitStream<WebSocketStream<TokioIo<Upgraded>>>);

/// Represents a single WebSocket event produced by [`WsStream::recv`].
///
/// WebSocket communication includes both **data** messages and **control** messages.
/// In split mode, control messages (such as `Close`) must be surfaced so the caller can
/// coordinate protocol-correct behavior (e.g. echoing the close frame via [`WsSink::close`]).
///
/// - [`WsEvent::Data`] contains an application-level message deserialized from an incoming
///   WebSocket data frame (text or binary).
/// - [`WsEvent::Close`] is emitted when a close control message is received. After this event
///   the caller should typically reply with [`WsSink::close`] and stop reading.
#[derive(Debug)]
pub enum WsEvent<T> {
    /// Application-level message deserialized from an incoming data frame.
    Data(T),

    /// A close control message received from the peer.
    ///
    /// The contained [`CloseFrame`] (if any) carries the close code and reason.
    Close(Option<CloseFrame>),
}

impl std::fmt::Debug for WsSink {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WsSink(..)")
    }
}

impl std::fmt::Debug for WsStream {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("WsStream(..)")
    }
}

impl WsSink {
    /// Unwraps the inner [`Sink`]
    #[inline]
    pub fn into_inner(self) -> SplitSink<WebSocketStream<TokioIo<Upgraded>>, WsMessage> {
        self.0
    }

    /// Sends a message to the peer.
    ///
    /// The message type `T` is converted into a WebSocket [`Message`] via [`TryInto`].
    ///
    /// This method is intended for application data (text/binary), but may also be used to send
    /// control messages if your higher-level protocol requires it.
    ///
    /// # Errors
    /// Returns an error if message conversion fails or if the underlying sink fails to send.
    #[inline]
    pub async fn send<T: TryInto<Message, Error = Error>>(&mut self, msg: T) -> Result<(), Error> {
        let msg = msg.try_into()?.into();
        self.0.send(msg)
            .await
            .map_err(Error::from)
    }

    /// Completes the close handshake and closes the sink.
    ///
    /// This method first attempts to send a `Close` control message to the peer, echoing the
    /// provided `frame` if present, and then closes the underlying sink.
    ///
    /// Typical usage is to call this after [`WsStream::recv`] yields [`WsEvent::Close`].
    #[inline]
    pub async fn close(&mut self, frame: Option<CloseFrame>) -> Result<(), Error> {
        self.0.send(tungstenite::Message::Close(frame)).await?;
        match self.0.close().await {
            Ok(()) => Ok(()),
            Err(e) if is_expected_close_error(&e) => Ok(()),
            Err(e) => Err(Error::from(e)),
        }
    }
}

impl WsStream {
    /// Unwraps the inner [`Stream`]
    #[inline]
    pub fn into_inner(self) -> SplitStream<WebSocketStream<TokioIo<Upgraded>>> {
        self.0
    }

    /// Receives the next WebSocket event.
    ///
    /// This method yields:
    /// - [`WsEvent::Data`] for text/binary frames that successfully deserialize into `T`.
    /// - [`WsEvent::Close`] when a close control message is received.
    ///
    /// Ping/pong frames are ignored (they are handled at the protocol level by the underlying
    /// WebSocket implementation).
    ///
    /// # Errors
    /// Returns an error if the underlying stream fails, or if deserializing a data frame into `T`
    /// fails.
    ///
    /// # Close behavior
    /// On [`WsEvent::Close`], callers should typically respond by calling [`WsSink::close`] with
    /// the received frame and then stop reading.
    pub async fn recv<T>(&mut self) -> Option<Result<WsEvent<T>, Error>>
    where
        T: TryFrom<Message, Error = Error>,
    {
        loop {
            let msg = match self.recv_raw().await? { 
                Ok(msg) => msg,
                Err(err) => return Some(Err(err))
            };

            match msg.0 {
                WsMessage::Ping(_) | WsMessage::Pong(_) => continue,
                WsMessage::Close(frame) => return Some(Ok(WsEvent::Close(frame))),
                WsMessage::Text(_) | WsMessage::Binary(_) => {
                    return Some(T::try_from(msg).map(WsEvent::Data));
                },
                WsMessage::Frame(_) => {
                    debug_assert!(
                        false,
                        "tungstenite returned a raw Frame while reading messages"
                    );
                    continue;
                },
            }
        }
    }

    /// Receives a raw [`Message`]
    #[inline]
    async fn recv_raw(&mut self) -> Option<Result<Message, Error>> {
        recv_raw_from(&mut self.0).await
    }
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

    /// Receives the next application message.
    ///
    /// This is a data-oriented API: it yields messages deserialized from text/binary frames into `T`.
    /// Ping/pong frames are ignored.
    ///
    /// If a close control message is received, this method attempts a graceful close and then ends
    /// the stream by returning `None`.
    ///
    /// # Errors
    /// Returns an error if the underlying socket fails, or if deserializing a data frame into `T`
    /// fails.
    pub async fn recv<T>(&mut self) -> Option<Result<T, Error>>
    where
        T: TryFrom<Message, Error = Error>,
    {
        loop {
            let msg = match self.recv_raw().await? {
                Ok(msg) => msg,
                Err(err) => return Some(Err(err))
            };

            match msg.0 {
                WsMessage::Ping(_) | WsMessage::Pong(_) => continue,
                WsMessage::Text(_) | WsMessage::Binary(_) => return Some(T::try_from(msg)),
                WsMessage::Frame(_) => {
                    debug_assert!(
                        false,
                        "tungstenite returned a raw Frame while reading messages"
                    );
                    continue;
                },
                WsMessage::Close(frame) => {
                    if let Err(_close_err) = self.close(frame).await {
                        #[cfg(feature = "tracing")]
                        tracing::warn!("WebSocket close failed: {_close_err}");
                    }
                    return None;
                }
            }
        }
    }

    /// Sends a message to the peer.
    ///
    /// The message type `T` is converted into a WebSocket [`Message`] via [`TryInto`].
    ///
    /// # Errors
    /// Returns an error if message conversion fails or if the underlying sink fails to send.
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
                    continue;
                }
            };

            let response = handler(msg).await;
            
            if let Err(_e) = self.send(response).await {
                #[cfg(feature = "tracing")]
                tracing::error!("Error sending message: {_e}");

                if let Err(_close_err) = self.close(None).await {
                    #[cfg(feature = "tracing")]
                    tracing::warn!("WebSocket close failed: {_close_err}");
                }
                
                return;
            }
        }
    }

    /// Closes the WebSocket connection.
    ///
    /// This attempts to perform a graceful close handshake using the provided close `frame`
    /// (if any). If the close handshake fails, the error is logged when `tracing` is enabled.
    #[inline]
    pub async fn close(&mut self, frame: Option<CloseFrame>) -> Result<(), Error> {
        match self.inner.close(frame).await {
            Ok(()) => Ok(()),
            Err(e) if is_expected_close_error(&e) => Ok(()),
            Err(e) => Err(Error::from(e)),
        }
    }

    /// Receives a raw [`Message`]
    #[inline]
    async fn recv_raw(&mut self) -> Option<Result<Message, Error>> {
        recv_raw_from(&mut self.inner).await
    }
}

/// Receives the next raw WebSocket message from any tungstenite-backed stream.
#[inline]
async fn recv_raw_from<S>(stream: &mut S) -> Option<Result<Message, Error>>
where
    S: Stream<Item = Result<WsMessage, tungstenite::Error>> + Unpin,
{
    stream
        .next()
        .await
        .map(|r| 
            r.map(Message).map_err(Error::from)
        )
}

#[inline]
fn is_expected_close_error(e: &WsError) -> bool {
    match e {
        WsError::ConnectionClosed => true,
        WsError::AlreadyClosed => true,
        WsError::Protocol(p) => matches!(
            p, 
            tungstenite::error::ProtocolError::SendAfterClosing
        ),
        WsError::Io(io) => matches!(
            io.kind(),
            std::io::ErrorKind::BrokenPipe
                | std::io::ErrorKind::ConnectionReset
                | std::io::ErrorKind::NotConnected
        ),
        _ => false,
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
                    let WsMessage::Frame(_) = msg else { return Poll::Ready(Some(Ok(Message(msg)))) };
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
