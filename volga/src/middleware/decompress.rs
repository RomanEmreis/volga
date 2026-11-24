//! Decompression middleware
//!
//! Middleware that decompresses the HTTP request body. A body that is not a valid stream of
//! its `Content-Encoding` is answered with `400 Bad Request`, and so is a zstd frame declaring
//! a window over 8 MiB, the limit RFC 9659 sets for the `zstd` content coding.

#[cfg(feature = "decompression-brotli")]
use async_compression::tokio::bufread::BrotliDecoder;

#[cfg(feature = "decompression-gzip")]
use async_compression::tokio::bufread::{GzipDecoder, ZlibDecoder};

#[cfg(feature = "decompression-zstd")]
use async_compression::{tokio::bufread::ZstdDecoder, zstd::DParameter};

use crate::{
    App, HttpBody, HttpRequestMut, HttpResult,
    error::Error,
    headers::{
        ACCEPT_ENCODING, CONTENT_ENCODING, CONTENT_LENGTH, ContentEncoding, Encoding, Header, VARY,
    },
    http::StatusCode,
    middleware::{HttpContext, NextFn},
    routing::{Route, RouteGroup},
    status,
};
use futures_util::{TryStream, TryStreamExt, future::ready};
use http_body_util::StreamBody;
use hyper::body::Frame;
use std::fmt::Display;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use tokio_util::io::{ReaderStream, StreamReader};

pub(crate) use limits::ResolvedDecompressionLimits;
pub use limits::{DecompressionLimits, ExpansionRatio};

mod limits;

static SUPPORTED_ENCODINGS: &[Encoding] = &[
    Encoding::Identity,
    #[cfg(feature = "decompression-brotli")]
    Encoding::Brotli,
    #[cfg(feature = "decompression-gzip")]
    Encoding::Gzip,
    #[cfg(feature = "decompression-gzip")]
    Encoding::Deflate,
    #[cfg(feature = "decompression-zstd")]
    Encoding::Zstd,
];

/// Represents current decompression's state
#[derive(Debug, Default)]
struct DecompressionState {
    compressed_bytes: AtomicUsize,
    decompressed_bytes: AtomicUsize,
}

impl DecompressionState {
    #[inline(always)]
    fn add_compressed(&self, n: usize) -> usize {
        self.compressed_bytes.fetch_add(n, Ordering::Relaxed) + n
    }
    #[inline(always)]
    fn add_decompressed(&self, n: usize) -> usize {
        self.decompressed_bytes.fetch_add(n, Ordering::Relaxed) + n
    }
    #[inline(always)]
    fn compressed(&self) -> usize {
        self.compressed_bytes.load(Ordering::Relaxed)
    }
}

/// The largest window a zstd frame may declare, as a power of two: 2^23, 8 MiB, the limit
/// RFC 9659 sets for the `zstd` content coding. zstd's own default is 2^27, 128 MiB, and the
/// decoder sizes its buffers from the window in the frame header, before any output exists for
/// the decompression limits to count.
#[cfg(feature = "decompression-zstd")]
const ZSTD_WINDOW_LOG_MAX: u32 = 23;

macro_rules! impl_decompressor {
    ($algo:ident, |$reader:ident| $decoder:expr) => {
        #[inline]
        fn $algo(body: HttpBody, limits: ResolvedDecompressionLimits) -> HttpBody {
            let state = Arc::new(DecompressionState::default());
            let body_stream = limited_compressed_stream(body, limits, state.clone());
            let $reader = StreamReader::new(body_stream);
            let decoder = $decoder;
            let decompressed_body =
                limited_decompressed_stream(ReaderStream::new(decoder), limits, state);

            HttpBody::boxed(StreamBody::new(
                decompressed_body.map_err(Error::from).map_ok(Frame::data),
            ))
        }
    };
}

#[cfg(feature = "decompression-brotli")]
impl_decompressor!(brotli, |reader| BrotliDecoder::new(reader));

#[cfg(feature = "decompression-gzip")]
impl_decompressor!(gzip, |reader| {
    let mut decoder = GzipDecoder::new(reader);
    decoder.multiple_members(true);
    decoder
});

#[cfg(feature = "decompression-gzip")]
impl_decompressor!(deflate, |reader| ZlibDecoder::new(reader));

#[cfg(feature = "decompression-zstd")]
impl_decompressor!(zstd, |reader| ZstdDecoder::with_params(
    reader,
    &[DParameter::window_log_max(ZSTD_WINDOW_LOG_MAX)]
));

impl App {
    /// Configures limits for the decompression middleware.
    pub fn with_decompression_limits<F>(self, configure: F) -> Self
    where
        F: FnOnce(DecompressionLimits) -> DecompressionLimits,
    {
        let limits = configure(self.decompression_limits);
        self.set_decompression_limits(limits)
    }

    /// Replaces the limits of the decompression middleware.
    ///
    /// The value form of [`with_decompression_limits`](App::with_decompression_limits), for
    /// limits built elsewhere. Unlike the closure, which starts from the limits already set,
    /// it replaces them all.
    ///
    /// # Example
    /// ```no_run
    /// use volga::{App, Limit, middleware::decompress::DecompressionLimits};
    ///
    /// let limits = DecompressionLimits::default()
    ///     .with_max_decompressed(Limit::Limited(8 * 1024 * 1024));
    /// let app = App::new().set_decompression_limits(limits);
    /// ```
    pub fn set_decompression_limits(mut self, limits: DecompressionLimits) -> Self {
        self.decompression_limits = limits;
        self
    }

    /// Registers a middleware that applies a default decompression algorithm
    pub fn use_decompression(&mut self) -> &mut Self {
        self.wrap(make_decompression_fn)
    }
}

impl<'a> RouteGroup<'a> {
    /// Registers a middleware that applies a default decompression algorithm for this group of routes
    pub fn with_decompression(&mut self) -> &mut Self {
        self.attach(make_decompression_fn)
    }
}

impl<'a> Route<'a> {
    /// Registers a middleware that applies a default decompression algorithm for this route
    pub fn with_decompression(self) -> Self {
        self.wrap(make_decompression_fn)
    }
}

async fn make_decompression_fn(mut ctx: HttpContext, next: NextFn) -> HttpResult {
    if let Ok(content_encoding) = ctx.extract::<Header<ContentEncoding>>() {
        let limits = ctx
            .request()
            .scope()
            .map(|s| s.decompression_limits)
            .unwrap_or_else(|| DecompressionLimits::default().resolved());

        match content_encoding.into_inner().try_into() {
            Ok(encoding) => {
                let (req, handler, cors) = ctx.into_parts();
                let req = decompress(encoding, req, limits);
                ctx = HttpContext::from_parts(req, handler, cors);
            }
            Err(error) if error.is_client_error() => (),
            Err(_) => {
                return status!(415; [
                    (VARY, CONTENT_ENCODING),
                    (ACCEPT_ENCODING, Encoding::stringify(SUPPORTED_ENCODINGS))
                ]);
            }
        }
    }
    next(ctx).await
}

fn decompress(
    encoding: Encoding,
    mut request: HttpRequestMut,
    limits: ResolvedDecompressionLimits,
) -> HttpRequestMut {
    let headers = request.headers_mut();
    headers.remove(CONTENT_LENGTH);
    headers.remove(CONTENT_ENCODING);

    request.map_body(|body| decompress_body(encoding, body, limits))
}

#[inline]
fn decompress_body(
    encoding: Encoding,
    body: HttpBody,
    limits: ResolvedDecompressionLimits,
) -> HttpBody {
    match encoding {
        #[cfg(feature = "decompression-brotli")]
        Encoding::Brotli => brotli(body, limits),
        #[cfg(feature = "decompression-gzip")]
        Encoding::Gzip => gzip(body, limits),
        #[cfg(feature = "decompression-gzip")]
        Encoding::Deflate => deflate(body, limits),
        #[cfg(feature = "decompression-zstd")]
        Encoding::Zstd => zstd(body, limits),
        _ => body,
    }
}

#[inline]
fn limited_compressed_stream(
    body: HttpBody,
    limits: ResolvedDecompressionLimits,
    state: Arc<DecompressionState>,
) -> impl TryStream<Ok = bytes::Bytes, Error = Error, Item = Result<bytes::Bytes, Error>> {
    body.into_data_stream().and_then(move |chunk| {
        let total = state.add_compressed(chunk.len());
        ready(
            check_max(
                total,
                limits.max_compressed_bytes,
                DecompressionError::CompressedBodyTooLarge,
            )
            .map(|_| chunk),
        )
    })
}

#[inline]
fn limited_decompressed_stream<R>(
    stream: R,
    limits: ResolvedDecompressionLimits,
    state: Arc<DecompressionState>,
) -> impl TryStream<Ok = bytes::Bytes, Error = Error>
where
    R: TryStream<Ok = bytes::Bytes, Error = std::io::Error>,
{
    stream.map_err(decoder_error).and_then(move |chunk| {
        let decompressed = state.add_decompressed(chunk.len());
        let compressed = state.compressed();

        let res = check_max(
            decompressed,
            limits.max_decompressed_bytes,
            DecompressionError::DecompressedBodyTooLarge,
        )
        .and_then(|_| check_ratio(decompressed, compressed, limits.max_expansion_ratio))
        .map(|_| chunk);

        ready(res)
    })
}

/// Turns an error of the decoder into the error of the body
///
/// What the decoder passes on from the compressed side - a decompression limit, the body
/// limit, a body that failed to arrive - comes wrapped by `StreamReader` and keeps its status.
/// Anything else the decoder raised itself, on a body that is not a valid stream of its
/// encoding: garbage or a stream cut short, a client error whatever `ErrorKind` the codec
/// gives it (`Other`, `UnexpectedEof` or `InvalidData`, depending on the codec and the fault).
#[inline]
fn decoder_error(err: std::io::Error) -> Error {
    match err.into_inner().map(|inner| inner.downcast::<Error>()) {
        Some(Ok(error)) => *error,
        _ => DecompressionError::InvalidCompressedBody.into(),
    }
}

#[inline]
fn check_max(total: usize, limit: Option<usize>, kind: DecompressionError) -> Result<(), Error> {
    if limit.is_some_and(|l| total > l) {
        Err(kind.into())
    } else {
        Ok(())
    }
}

#[inline]
fn check_ratio(
    decompressed: usize,
    compressed: usize,
    ratio: Option<ExpansionRatio>,
) -> Result<(), Error> {
    if let Some(r) = ratio {
        let allowed = compressed
            .saturating_mul(r.ratio)
            .saturating_add(r.slack_bytes);

        if decompressed > allowed {
            return Err(DecompressionError::ExpansionRatioExceeded.into());
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy)]
enum DecompressionError {
    CompressedBodyTooLarge,
    DecompressedBodyTooLarge,
    ExpansionRatioExceeded,
    InvalidCompressedBody,
}

impl Display for DecompressionError {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl crate::error::IntoError for DecompressionError {
    #[inline]
    fn into_error(self) -> Error {
        let status = match self {
            Self::InvalidCompressedBody => StatusCode::BAD_REQUEST,
            Self::CompressedBodyTooLarge
            | Self::DecompressedBodyTooLarge
            | Self::ExpansionRatioExceeded => StatusCode::PAYLOAD_TOO_LARGE,
        };
        Error::from_parts(status, None, format!("Decompression error: {self}"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HttpBody, Limit};
    use bytes::Bytes;
    use http_body_util::BodyExt;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    #[cfg(feature = "decompression-brotli")]
    async fn it_decompress_brotli() {
        use async_compression::tokio::write::BrotliEncoder;

        let data = b"{\"age\":33,\"name\":\"John\"}";
        let mut encoder = BrotliEncoder::new(Vec::new());
        encoder.write_all(data).await.unwrap();
        encoder.shutdown().await.unwrap();
        let compressed = encoder.into_inner();

        let body = HttpBody::full(compressed);
        let body = brotli(body, DecompressionLimits::default().resolved());

        assert_eq!(
            body.collect().await.unwrap().to_bytes(),
            Bytes::from_static(b"{\"age\":33,\"name\":\"John\"}")
        );
    }

    /// A large-window stream (marker `0x11`, a 1 GiB window, an empty last meta-block) is a
    /// malformed body: RFC 7932, which `br` names, has windows of at most 16 MiB
    #[tokio::test]
    #[cfg(feature = "decompression-brotli")]
    async fn it_rejects_large_window_brotli() {
        let body = HttpBody::full(&[0x11, 0xde][..]);
        let body = brotli(body, DecompressionLimits::default().resolved());

        let error = body.collect().await.unwrap_err();
        assert_eq!(error.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    #[cfg(feature = "decompression-gzip")]
    async fn it_decompress_gzip() {
        use async_compression::tokio::write::GzipEncoder;

        let data = b"{\"age\":33,\"name\":\"John\"}";
        let mut encoder = GzipEncoder::new(Vec::new());
        encoder.write_all(data).await.unwrap();
        encoder.shutdown().await.unwrap();
        let compressed = encoder.into_inner();

        let body = HttpBody::full(compressed);
        let body = gzip(body, DecompressionLimits::default().resolved());

        assert_eq!(
            body.collect().await.unwrap().to_bytes(),
            Bytes::from_static(b"{\"age\":33,\"name\":\"John\"}")
        );
    }

    #[tokio::test]
    #[cfg(feature = "decompression-gzip")]
    async fn it_decompress_deflate() {
        use async_compression::tokio::write::ZlibEncoder;

        let data = b"{\"age\":33,\"name\":\"John\"}";
        let mut encoder = ZlibEncoder::new(Vec::new());
        encoder.write_all(data).await.unwrap();
        encoder.shutdown().await.unwrap();
        let compressed = encoder.into_inner();

        let body = HttpBody::full(compressed);
        let body = deflate(body, DecompressionLimits::default().resolved());

        assert_eq!(
            body.collect().await.unwrap().to_bytes(),
            Bytes::from_static(b"{\"age\":33,\"name\":\"John\"}")
        );
    }

    #[tokio::test]
    #[cfg(feature = "decompression-zstd")]
    async fn it_decompress_zstd() {
        use async_compression::tokio::write::ZstdEncoder;

        let data = b"{\"age\":33,\"name\":\"John\"}";
        let mut encoder = ZstdEncoder::new(Vec::new());
        encoder.write_all(data).await.unwrap();
        encoder.shutdown().await.unwrap();
        let compressed = encoder.into_inner();

        let body = HttpBody::full(compressed);
        let body = zstd(body, DecompressionLimits::default().resolved());

        assert_eq!(
            body.collect().await.unwrap().to_bytes(),
            Bytes::from_static(b"{\"age\":33,\"name\":\"John\"}")
        );
    }

    #[tokio::test]
    #[cfg(feature = "decompression-brotli")]
    async fn it_decompress_with_max_compressed() {
        use async_compression::tokio::write::BrotliEncoder;

        let data = b"{\"age\":33,\"name\":\"John\"}";
        let mut encoder = BrotliEncoder::new(Vec::new());
        encoder.write_all(data).await.unwrap();
        encoder.shutdown().await.unwrap();
        let compressed = encoder.into_inner();

        let body = HttpBody::full(compressed);
        let body = brotli(
            body,
            DecompressionLimits::default()
                .with_max_compressed(Limit::Limited(1))
                .resolved(),
        );

        assert!(body.collect().await.is_err());
    }

    #[tokio::test]
    #[cfg(feature = "decompression-brotli")]
    async fn it_decompress_with_max_decompressed() {
        use async_compression::tokio::write::BrotliEncoder;

        let data = b"{\"age\":33,\"name\":\"John\"}";
        let mut encoder = BrotliEncoder::new(Vec::new());
        encoder.write_all(data).await.unwrap();
        encoder.shutdown().await.unwrap();
        let compressed = encoder.into_inner();

        let body = HttpBody::full(compressed);
        let body = brotli(
            body,
            DecompressionLimits::default()
                .with_max_decompressed(Limit::Limited(1))
                .resolved(),
        );

        assert!(body.collect().await.is_err());
    }

    #[tokio::test]
    #[cfg(feature = "decompression-brotli")]
    async fn it_decompress_with_max_expansion_ratio() {
        use async_compression::tokio::write::BrotliEncoder;

        let data = vec![b'a'; 64 * 1024];
        let mut encoder = BrotliEncoder::new(Vec::new());
        encoder.write_all(&data).await.unwrap();
        encoder.shutdown().await.unwrap();
        let compressed = encoder.into_inner();

        let body = HttpBody::full(compressed);
        let body = brotli(
            body,
            DecompressionLimits::default()
                .with_max_expansion_ratio(ExpansionRatio::new(1, 0))
                .resolved(),
        );

        assert!(body.collect().await.is_err());
    }

    #[test]
    fn it_keeps_the_status_of_an_error_passed_on_by_the_decoder() {
        let passed_on =
            std::io::Error::from(Error::from(DecompressionError::CompressedBodyTooLarge));
        let error = decoder_error(passed_on);

        assert_eq!(error.status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert_eq!(
            error.to_string(),
            "Decompression error: CompressedBodyTooLarge"
        );
    }

    #[test]
    fn it_answers_400_to_an_error_of_the_decoder() {
        let errors = [
            std::io::Error::other("Unknown frame descriptor"),
            std::io::Error::from(std::io::ErrorKind::UnexpectedEof),
            std::io::Error::new(std::io::ErrorKind::InvalidData, "invalid gzip header"),
        ];

        for err in errors {
            let error = decoder_error(err);
            assert_eq!(error.status(), StatusCode::BAD_REQUEST);
            assert_eq!(
                error.to_string(),
                "Decompression error: InvalidCompressedBody"
            );
        }
    }

    #[test]
    fn it_sets_decompression_limit_by_default() {
        let app = App::new();

        assert_eq!(
            app.decompression_limits.max_compressed_bytes,
            Limit::Limited(5 * 1024 * 1024)
        );
        assert_eq!(
            app.decompression_limits.max_decompressed_bytes,
            Limit::Limited(16 * 1024 * 1024)
        );
        assert_eq!(
            app.decompression_limits.max_expansion_ratio,
            Some(ExpansionRatio::new(100, 1024 * 1024))
        );
    }

    #[test]
    fn it_replaces_decompression_limits() {
        let app = App::new()
            .with_decompression_limits(|limits| limits.with_max_compressed(Limit::Limited(1024)))
            .set_decompression_limits(
                DecompressionLimits::default().with_max_decompressed(Limit::Limited(2048)),
            );

        assert_eq!(
            app.decompression_limits.max_compressed_bytes,
            Limit::Limited(5 * 1024 * 1024)
        );
        assert_eq!(
            app.decompression_limits.max_decompressed_bytes,
            Limit::Limited(2048)
        );
    }

    #[test]
    fn it_configures_decompression_limits_over_the_ones_set() {
        let app = App::new()
            .set_decompression_limits(
                DecompressionLimits::default().with_max_decompressed(Limit::Limited(2048)),
            )
            .with_decompression_limits(|limits| limits.with_max_compressed(Limit::Limited(1024)));

        assert_eq!(
            app.decompression_limits.max_compressed_bytes,
            Limit::Limited(1024)
        );
        assert_eq!(
            app.decompression_limits.max_decompressed_bytes,
            Limit::Limited(2048)
        );
    }
}
