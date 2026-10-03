//! Lingering close: a connection the server closes while the client may still be sending is
//! read from until the client is done, so that the response it was sent reaches it
//!
//! An HTTP/1 connection whose request body was not read to the end cannot be reused, so
//! hyper closes it once the response is out - a `413` for a body over the limit, a `401`
//! refusing an upload, anything answered without reading the body. The client may still be
//! sending that body. A socket closed with data still arriving makes the kernel answer with a
//! TCP reset, and a reset can overtake the response: the client sees "connection reset" and
//! never reads the status it was sent.
//!
//! So the socket is not closed when hyper is done with it. [`LingeringStream`] hands it back
//! to the connection task instead, which shuts the write side down - the client reads the
//! response, then the end of the stream - and reads and throws away whatever still arrives
//! until the client closes its side, for [`LINGER_TIMEOUT`] at most. A connection the client
//! closed first has nothing left to arrive and is closed at once, which is how nearly every
//! connection ends.
//!
//! Lingering also ends once the client has sent nothing for [`LINGER_IDLE_TIMEOUT`], as
//! nginx's `lingering_timeout` does. A client still sending a body keeps it going; one that
//! holds an idle connection and does not watch it - closed by the server on a graceful
//! shutdown, which waits for lingering connections as well - would otherwise hold it, and
//! the shutdown, for all of [`LINGER_TIMEOUT`].
//!
//! HTTP/2 does not get there on a body left unread: a response sent early ends the stream
//! alone, with `RST_STREAM(NO_ERROR)` (RFC 9113 Section 8.1), and the connection stays open.
//! An HTTP/2 connection lingers only when the server closes it as a whole - once a graceful
//! shutdown has sent its `GOAWAY`, or on a connection error - so that a client still sending
//! frames reads that `GOAWAY`, and learns which requests to retry, rather than a reset.
//!
//! Nothing here is gated on the protocol. With both enabled, the protocol is told from the
//! first bytes the client sends, which is after the socket is wrapped, and a connection the
//! client closes first - nearly every one, in either protocol - costs only the one allocation
//! of the channel it would have been handed back through.

use std::{
    io,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt, ReadBuf},
    net::TcpStream,
    sync::oneshot,
    time::{Instant, timeout},
};

/// How long a connection the server closed is read from, at most, while the client finishes
/// sending and closes its side
const LINGER_TIMEOUT: Duration = Duration::from_secs(2);

/// How long the client may send nothing, at most, before the server stops waiting for it
const LINGER_IDLE_TIMEOUT: Duration = Duration::from_millis(500);

/// The buffer what still arrives is read into and thrown away from
const LINGER_BUFFER_SIZE: usize = 16 * 1024;

/// A connection's socket that is handed back to the connection task, rather than closed, if
/// it is dropped while the client may still be sending
#[derive(Debug)]
pub(crate) struct LingeringStream {
    /// `None` only once dropped
    stream: Option<TcpStream>,
    /// Set once the client has closed its side: nothing is left to arrive, so nothing is left
    /// to linger for
    peer_closed: bool,
    hand_back: Option<oneshot::Sender<TcpStream>>,
}

/// The connection task's side of a [`LingeringStream`]
#[derive(Debug)]
pub(crate) struct Linger(oneshot::Receiver<TcpStream>);

impl LingeringStream {
    /// Wraps `stream`, along with the [`Linger`] that closes it once the connection is done
    pub(crate) fn new(stream: TcpStream) -> (Self, Linger) {
        let (hand_back, linger) = oneshot::channel();
        let stream = Self {
            stream: Some(stream),
            peer_closed: false,
            hand_back: Some(hand_back),
        };

        (stream, Linger(linger))
    }

    #[inline]
    fn stream(&mut self) -> io::Result<Pin<&mut TcpStream>> {
        self.stream
            .as_mut()
            .map(Pin::new)
            .ok_or_else(|| io::ErrorKind::NotConnected.into())
    }
}

impl Drop for LingeringStream {
    fn drop(&mut self) {
        if self.peer_closed {
            return;
        }

        if let (Some(stream), Some(hand_back)) = (self.stream.take(), self.hand_back.take()) {
            // Nobody waits for it once the connection task is gone - a connection closed
            // when the shutdown ran out of time - and the socket is closed right here
            let _ = hand_back.send(stream);
        }
    }
}

impl AsyncRead for LingeringStream {
    #[inline]
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = self.get_mut();
        let room = buf.remaining();
        let filled = buf.filled().len();

        let res = this.stream()?.poll_read(cx, buf);
        match &res {
            // Nothing read into a buffer with room for it is the end of the stream
            Poll::Ready(Ok(())) if room > 0 && buf.filled().len() == filled => {
                this.peer_closed = true
            }
            // A connection that failed has nothing left to receive either
            Poll::Ready(Err(_)) => this.peer_closed = true,
            _ => (),
        }
        res
    }
}

impl AsyncWrite for LingeringStream {
    #[inline]
    fn poll_write(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        self.get_mut().stream()?.poll_write(cx, buf)
    }

    #[inline]
    fn poll_write_vectored(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        self.get_mut().stream()?.poll_write_vectored(cx, bufs)
    }

    #[inline]
    fn is_write_vectored(&self) -> bool {
        self.stream.as_ref().is_some_and(|s| s.is_write_vectored())
    }

    #[inline]
    fn poll_flush(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.get_mut().stream()?.poll_flush(cx)
    }

    #[inline]
    fn poll_shutdown(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        self.get_mut().stream()?.poll_shutdown(cx)
    }
}

impl Linger {
    /// Closes the connection once the client is done sending, if the server closed it first
    ///
    /// Called once the connection is served. A socket still in use at that point - by a
    /// WebSocket the connection was upgraded to - is left to whatever uses it, and closes when
    /// it is dropped.
    #[inline]
    pub(crate) async fn close(self) {
        self.close_within(LINGER_TIMEOUT, LINGER_IDLE_TIMEOUT).await
    }

    /// Lingers for `total` at most, and for `idle` at most since the client last sent anything
    async fn close_within(mut self, total: Duration, idle: Duration) {
        let Ok(mut stream) = self.0.try_recv() else {
            return;
        };

        // Most likely shut down already, once the response went out. If not - the connection
        // failed, say - the client is told now that nothing more is coming
        let _ = stream.shutdown().await;

        let deadline = Instant::now() + total;
        let mut discard = vec![0; LINGER_BUFFER_SIZE];

        // A read that is ready is taken even once the time is up, so the deadline is checked
        // here as well as waited on: a client sending without a pause would never let it pass
        while let Some(left) = deadline.checked_duration_since(Instant::now())
            && !left.is_zero()
        {
            match timeout(idle.min(left), stream.read(&mut discard)).await {
                Ok(Ok(read)) if read > 0 => continue,
                // The client closed, the connection failed, or nothing arrived in time
                _ => break,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;

    /// A connected pair: the server's side wrapped, along with its `Linger`, and the client's
    async fn connected() -> (LingeringStream, Linger, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let client = TcpStream::connect(listener.local_addr().unwrap())
            .await
            .unwrap();
        let (server, _) = listener.accept().await.unwrap();
        let (server, linger) = LingeringStream::new(server);

        (server, linger, client)
    }

    #[tokio::test]
    async fn it_hands_the_socket_back_if_the_client_has_not_closed() {
        let (server, mut linger, _client) = connected().await;

        drop(server);

        assert!(linger.0.try_recv().is_ok());
    }

    #[tokio::test]
    async fn it_closes_at_once_once_the_client_has_closed() {
        let (mut server, mut linger, client) = connected().await;

        drop(client);
        let mut buf = [0; 8];
        assert_eq!(server.read(&mut buf).await.unwrap(), 0);
        drop(server);

        assert!(linger.0.try_recv().is_err());
    }

    #[tokio::test]
    async fn it_reads_until_the_client_closes() {
        let (mut server, linger, mut client) = connected().await;

        server.write_all(b"HTTP/1.1 413").await.unwrap();
        drop(server);

        let lingering = tokio::spawn(linger.close());

        // The client is still sending, and reads the response and the end of the stream
        client.write_all(&[b'x'; 64 * 1024]).await.unwrap();
        let mut response = Vec::new();
        client.read_to_end(&mut response).await.unwrap();
        drop(client);

        assert_eq!(response, b"HTTP/1.1 413");
        timeout(Duration::from_secs(1), lingering)
            .await
            .expect("lingering ends once the client closes")
            .unwrap();
    }

    #[tokio::test]
    async fn it_stops_lingering_once_the_client_goes_quiet() {
        let (server, linger, _client) = connected().await;

        drop(server);

        // The client neither sends nor closes, so only the idle timeout ends it
        timeout(
            Duration::from_secs(1),
            linger.close_within(Duration::from_secs(10), Duration::from_millis(20)),
        )
        .await
        .expect("lingering ends once the client has gone quiet");
    }

    #[tokio::test]
    async fn it_stops_lingering_after_the_timeout_while_the_client_keeps_sending() {
        let (server, linger, mut client) = connected().await;

        drop(server);
        let sending =
            tokio::spawn(async move { while client.write_all(&[b'x'; 1024]).await.is_ok() {} });

        // The client never pauses, so only the total timeout ends it
        timeout(
            Duration::from_secs(1),
            linger.close_within(Duration::from_millis(50), Duration::from_secs(10)),
        )
        .await
        .expect("lingering ends once its time is up");

        sending.abort();
    }

    #[tokio::test]
    async fn it_keeps_lingering_while_the_client_sends_with_pauses() {
        let (server, linger, mut client) = connected().await;

        drop(server);
        let lingering =
            tokio::spawn(linger.close_within(Duration::from_secs(10), Duration::from_millis(200)));

        // Pauses shorter than the idle timeout keep the server reading, so every write lands
        for _ in 0..10 {
            client.write_all(&[b'x'; 1024]).await.unwrap();
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        drop(client);

        timeout(Duration::from_secs(1), lingering)
            .await
            .expect("lingering ends once the client closes")
            .unwrap();
    }
}
