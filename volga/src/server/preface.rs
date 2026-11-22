//! A deadline on the bytes that tell HTTP/1 from HTTP/2.
//!
//! hyper's header read timeout starts when its HTTP/1 connection reads a request head, but a
//! server that speaks both protocols, or upgrades to WebSockets, first reads the start of the
//! connection to tell which protocol it is (`hyper_util::server::conn::auto`), with no timer.
//! A client that connects and sends nothing, or stops partway into what could still be the
//! HTTP/2 preface, holds the connection there for as long as it likes, past any header read
//! timeout and after a bounded TLS handshake. An HTTP/2 connection waits for the preface the
//! same way.
//!
//! [`PrefaceDeadline`] bounds that phase with the header read timeout. It follows what the
//! client sends against the HTTP/2 preface, and stops timing once the bytes differ from it
//! (HTTP/1, which hyper's own timer covers from there), once the whole preface has arrived
//! (HTTP/2), or at the end of the stream. What the connection does after that is not timed by
//! it, so neither a keep-alive connection nor a handler is affected. Each phase has the whole
//! timeout - the TLS handshake, telling the protocol and then hyper's timer on the head - so a
//! client that keeps every phase just under it finishes its first request head within three
//! times the timeout.
//!
//! The deadline is set when the stream is created. The timer that enforces it is made only
//! when a read has to wait for the client - at most once per connection, and not at all when
//! the first read already tells the protocol, as it does when a TLS stream holds the request
//! by the time the handshake ends. The bytes are looked at only until the protocol is known:
//! after that a read costs one branch.

use std::{
    future::Future,
    io,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};
use tokio::{
    io::{AsyncRead, AsyncWrite, ReadBuf},
    time::{Instant, Sleep, sleep_until},
};

/// What an HTTP/2 connection starts with (RFC 9113 Section 3.4)
const H2_PREFACE: &[u8] = b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n";

/// A stream that fails its reads with [`io::ErrorKind::TimedOut`] if the protocol is not known
/// within the timeout it was created with. See the module documentation.
pub(super) struct PrefaceDeadline<S> {
    io: S,
    /// When the protocol has to be known by; `None` once it is, or with no timeout
    deadline: Option<Instant>,
    /// The timer that wakes the connection at `deadline`, made the first time a read waits.
    /// `Sleep` is not `Unpin` and hyper takes an `Unpin` stream, hence the box
    timer: Option<Pin<Box<Sleep>>>,
    /// How many bytes of the HTTP/2 preface the client has sent so far
    matched: usize,
}

impl<S> PrefaceDeadline<S> {
    /// Wraps `io`, with no deadline if `timeout` is `None`
    #[inline]
    pub(super) fn new(io: S, timeout: Option<Duration>) -> Self {
        Self {
            io,
            deadline: timeout.map(|timeout| Instant::now() + timeout),
            timer: None,
            matched: 0,
        }
    }

    /// Follows the HTTP/2 preface through the bytes just `read`, which are not empty, and
    /// returns whether they tell the protocol
    fn tells_protocol(&mut self, read: &[u8]) -> bool {
        let end = (self.matched + read.len()).min(H2_PREFACE.len());
        if !read.starts_with(&H2_PREFACE[self.matched..end]) {
            return true;
        }

        self.matched = end;
        self.matched == H2_PREFACE.len()
    }
}

impl<S: AsyncRead + Unpin> AsyncRead for PrefaceDeadline<S> {
    #[inline]
    fn poll_read(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<io::Result<()>> {
        let this = &mut *self;
        let Some(deadline) = this.deadline else {
            return Pin::new(&mut this.io).poll_read(cx, buf);
        };

        let before = buf.filled().len();
        let capacity = buf.remaining();
        match Pin::new(&mut this.io).poll_read(cx, buf) {
            Poll::Ready(Ok(())) => {
                let read = &buf.filled()[before..];
                // An empty read into a buffer with room in it is the end of the stream
                let known = if read.is_empty() {
                    capacity > 0
                } else {
                    this.tells_protocol(read)
                };

                if known {
                    this.deadline = None;
                    this.timer = None;
                }

                Poll::Ready(Ok(()))
            }
            Poll::Ready(Err(err)) => Poll::Ready(Err(err)),
            Poll::Pending => {
                let timer = this
                    .timer
                    .get_or_insert_with(|| Box::pin(sleep_until(deadline)));
                if timer.as_mut().poll(cx).is_ready() {
                    return Poll::Ready(Err(io::Error::new(
                        io::ErrorKind::TimedOut,
                        "the client did not start a request in time",
                    )));
                }
                Poll::Pending
            }
        }
    }
}

impl<S: AsyncWrite + Unpin> AsyncWrite for PrefaceDeadline<S> {
    #[inline]
    fn poll_write(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &[u8],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.io).poll_write(cx, buf)
    }

    #[inline]
    fn poll_write_vectored(
        mut self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        bufs: &[io::IoSlice<'_>],
    ) -> Poll<io::Result<usize>> {
        Pin::new(&mut self.io).poll_write_vectored(cx, bufs)
    }

    #[inline]
    fn is_write_vectored(&self) -> bool {
        self.io.is_write_vectored()
    }

    #[inline]
    fn poll_flush(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.io).poll_flush(cx)
    }

    #[inline]
    fn poll_shutdown(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<io::Result<()>> {
        Pin::new(&mut self.io).poll_shutdown(cx)
    }
}

#[cfg(test)]
mod tests {
    use super::{H2_PREFACE, PrefaceDeadline};
    use std::time::Duration;
    use tokio::io::{AsyncReadExt, AsyncWriteExt, duplex};

    const TIMEOUT: Duration = Duration::from_millis(50);

    /// The index of the chunk that tells the protocol, if any does
    fn tells(chunks: &[&[u8]]) -> Option<usize> {
        let mut stream = PrefaceDeadline::new((), None);
        chunks.iter().position(|chunk| stream.tells_protocol(chunk))
    }

    #[test]
    fn it_tells_http1_on_the_first_byte_off_the_preface() {
        assert_eq!(tells(&[b"GET / HTTP/1.1\r\n"]), Some(0));
        assert_eq!(tells(&[b"P", b"OST / HTTP/1.1\r\n"]), Some(1));
        assert_eq!(tells(&[b"PRI * HTTP/1.1\r\n"]), Some(0));
    }

    #[test]
    fn it_tells_http2_once_the_whole_preface_arrives() {
        assert_eq!(tells(&[H2_PREFACE]), Some(0));
        assert_eq!(tells(&[&H2_PREFACE[..10], &H2_PREFACE[10..]]), Some(1));
        assert_eq!(
            tells(&[b"PRI * HTTP/2.0\r\n\r\nSM\r\n\r\n\x00\x00"]),
            Some(0)
        );
    }

    #[test]
    fn it_waits_on_a_part_of_the_preface() {
        assert_eq!(tells(&[b"P", b"RI * ", b"HTTP/2.0"]), None);
    }

    #[tokio::test]
    async fn it_times_out_a_silent_client() {
        let (_client, server) = duplex(64);
        let mut stream = PrefaceDeadline::new(server, Some(TIMEOUT));

        let err = stream.read(&mut [0u8; 16]).await.unwrap_err();

        assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
    }

    #[tokio::test]
    async fn it_times_out_a_client_that_stops_inside_the_preface() {
        let (mut client, server) = duplex(64);
        let mut stream = PrefaceDeadline::new(server, Some(TIMEOUT));
        let mut buf = [0u8; 16];

        client.write_all(b"PRI * ").await.unwrap();
        assert_eq!(stream.read(&mut buf).await.unwrap(), 6);

        let err = stream.read(&mut buf).await.unwrap_err();
        assert_eq!(err.kind(), std::io::ErrorKind::TimedOut);
    }

    #[tokio::test]
    async fn it_stops_timing_once_the_whole_preface_arrives() {
        let (mut client, server) = duplex(64);
        let mut stream = PrefaceDeadline::new(server, Some(TIMEOUT));
        let mut buf = [0u8; 64];

        client.write_all(H2_PREFACE).await.unwrap();
        assert_eq!(stream.read(&mut buf).await.unwrap(), H2_PREFACE.len());

        tokio::time::sleep(TIMEOUT * 3).await;
        client.write_all(b"\x00").await.unwrap();
        assert_eq!(stream.read(&mut buf).await.unwrap(), 1);
    }

    #[tokio::test]
    async fn it_makes_no_timer_when_the_first_read_tells_the_protocol() {
        let (mut client, server) = duplex(64);
        let mut stream = PrefaceDeadline::new(server, Some(TIMEOUT));

        client.write_all(b"GET / HTTP/1.1\r\n").await.unwrap();
        assert_eq!(stream.read(&mut [0u8; 64]).await.unwrap(), 16);

        assert!(stream.deadline.is_none());
        assert!(stream.timer.is_none());
    }

    #[tokio::test]
    async fn it_drops_the_timer_once_the_protocol_is_known() {
        let (mut client, server) = duplex(64);
        let mut stream = PrefaceDeadline::new(server, Some(TIMEOUT));
        let mut buf = [0u8; 16];

        let read = async { stream.read(&mut buf).await.unwrap() };
        let write = async {
            tokio::time::sleep(TIMEOUT / 5).await;
            client.write_all(b"GET").await.unwrap();
        };
        let (read, ()) = tokio::join!(read, write);

        assert_eq!(read, 3);
        assert!(stream.deadline.is_none());
        assert!(stream.timer.is_none());
    }

    #[tokio::test]
    async fn it_passes_the_end_of_the_stream_on() {
        let (client, server) = duplex(64);
        let mut stream = PrefaceDeadline::new(server, Some(TIMEOUT));
        drop(client);

        assert_eq!(stream.read(&mut [0u8; 16]).await.unwrap(), 0);
    }

    #[tokio::test]
    async fn it_stops_timing_once_the_protocol_is_known() {
        let (mut client, server) = duplex(64);
        let mut stream = PrefaceDeadline::new(server, Some(TIMEOUT));
        let mut buf = [0u8; 16];

        client.write_all(b"GET").await.unwrap();
        assert_eq!(stream.read(&mut buf).await.unwrap(), 3);

        tokio::time::sleep(TIMEOUT * 3).await;
        client.write_all(b" /").await.unwrap();
        assert_eq!(stream.read(&mut buf).await.unwrap(), 2);
    }

    #[tokio::test]
    async fn it_does_not_time_out_without_a_timeout() {
        let (mut client, server) = duplex(64);
        let mut stream = PrefaceDeadline::new(server, None);

        let read = async {
            let mut buf = [0u8; 16];
            stream.read(&mut buf).await.map(|n| buf[..n].to_vec())
        };
        let write = async {
            tokio::time::sleep(TIMEOUT * 3).await;
            client.write_all(b"P").await.unwrap();
        };
        let (read, ()) = tokio::join!(read, write);

        assert_eq!(read.unwrap(), b"P");
    }
}
