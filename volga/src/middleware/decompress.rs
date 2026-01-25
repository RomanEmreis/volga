//! Decompression middleware
//!
//! Middleware that decompresses the HTTP request body

#[cfg(feature = "decompression-brotli")]
use async_compression::tokio::bufread::BrotliDecoder;

#[cfg(feature = "decompression-gzip")]
use async_compression::tokio::bufread::{ZlibDecoder, GzipDecoder};

#[cfg(feature = "decompression-zstd")]
use async_compression::tokio::bufread::ZstdDecoder;

use futures_util::{TryStream, TryStreamExt, future::ready};
use http_body_util::StreamBody;
use hyper::body::Frame;
use tokio_util::io::{
    ReaderStream,
    StreamReader
};
use std::fmt::Display;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use crate::{
    App, 
    routing::{Route, RouteGroup}, 
    error::Error, 
    headers::{
        ContentEncoding,
        Header,
        Encoding,
        ACCEPT_ENCODING,
        CONTENT_ENCODING,
        CONTENT_LENGTH,
        VARY
    }, 
    http::{StatusCode, request::request_body_limit::RequestBodyLimit}, 
    middleware::{HttpContext, NextFn}, 
    HttpRequestMut,
    HttpRequest,
    HttpResult,
    HttpBody, 
    status, 
};

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

macro_rules! impl_decompressor {
    ($algo:ident, $decoder:ident, $mm:literal) => {
        #[inline]
        fn $algo(body: HttpBody, limits: ResolvedDecompressionLimits) -> HttpBody {
            let state = Arc::new(DecompressionState::default());
            let body_stream = limited_compressed_stream(body, limits, state.clone());
            let stream_reader = StreamReader::new(body_stream);
            let mut decoder = $decoder::new(stream_reader);
            decoder.multiple_members($mm);
            let decompressed_body = limited_decompressed_stream(ReaderStream::new(decoder), limits, state);
    
            HttpBody::boxed(StreamBody::new(decompressed_body
                .map_err(Error::from)
                .map_ok(Frame::data))
            )
        }
    };
}

#[cfg(feature = "decompression-brotli")]
impl_decompressor!(brotli, BrotliDecoder, false);

#[cfg(feature = "decompression-gzip")]
impl_decompressor!(gzip, GzipDecoder, true);

#[cfg(feature = "decompression-gzip")]
impl_decompressor!(deflate, ZlibDecoder, false);

#[cfg(feature = "decompression-zstd")]
impl_decompressor!(zstd, ZstdDecoder, false);

impl App {
    /// Configures limits for the decompression middleware.
    pub fn with_decompression_limits<F>(mut self, configure: F) -> Self
    where
        F: FnOnce(DecompressionLimits) -> DecompressionLimits,
    {
        self.decompression_limits = configure(self.decompression_limits);
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
        self.wrap(make_decompression_fn)
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
        let limits = ctx.request()
            .extensions()
            .get::<ResolvedDecompressionLimits>()
            .copied()
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
    request: HttpRequestMut,
    limits: ResolvedDecompressionLimits
) -> HttpRequestMut {
    let (mut parts, body) = request.into_parts();

    parts.headers.remove(CONTENT_LENGTH);
    parts.headers.remove(CONTENT_ENCODING);

    let body_limit = parts.extensions.get::<RequestBodyLimit>()
        .cloned()
        .unwrap_or_default();
    
    let body = decompress_body(encoding, body, limits);
    
    HttpRequestMut::new(
        HttpRequest::from_parts(parts, body).into_limited(body_limit)
    )
}

#[inline]
fn decompress_body(
    encoding: Encoding,
    body: HttpBody,
    limits: ResolvedDecompressionLimits
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
        _ => body
    }
}

#[inline]
fn limited_compressed_stream(
    body: HttpBody,
    limits: ResolvedDecompressionLimits,
    state: Arc<DecompressionState>
) -> impl TryStream<Ok = bytes::Bytes, Error = Error, Item = Result<bytes::Bytes, Error>> {
    body.into_data_stream()
        .and_then(move |chunk| {
            let total = state.add_compressed(chunk.len());
            ready(check_max(
                total, 
                limits.max_compressed_bytes, 
                DecompressionError::CompressedBodyTooLarge)
                .map(|_| chunk))
        })
}

#[inline]
fn limited_decompressed_stream<R>(
    stream: R,
    limits: ResolvedDecompressionLimits,
    state: Arc<DecompressionState>
) -> impl TryStream<Ok = bytes::Bytes, Error = Error>
where
    R: TryStream<Ok = bytes::Bytes, Error = std::io::Error>,
{
    stream
        .map_err(Into::into)
        .and_then(move |chunk| {
            let decompressed = state.add_decompressed(chunk.len());
            let compressed = state.compressed();

            let res = check_max(
                decompressed,
                limits.max_decompressed_bytes,
                DecompressionError::DecompressedBodyTooLarge)
                .and_then(|_| check_ratio(decompressed, compressed, limits.max_expansion_ratio))
                .map(|_| chunk);

            ready(res)
        })
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
}

impl Display for DecompressionError {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

impl From<DecompressionError> for Error {
    #[inline]
    fn from(err: DecompressionError) -> Self {
        Error::from_parts(
            StatusCode::PAYLOAD_TOO_LARGE,
            None,
            format!("Decompression error: {err}")
        )
    }
}

#[cfg(test)]
mod tests {
    use http_body_util::BodyExt;
    use bytes::Bytes;
    use tokio::io::AsyncWriteExt;
    use crate::{HttpBody, Limit};
    use super::*;

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
        
        assert_eq!(body.collect().await.unwrap().to_bytes(), Bytes::from_static(b"{\"age\":33,\"name\":\"John\"}"));
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

        assert_eq!(body.collect().await.unwrap().to_bytes(), Bytes::from_static(b"{\"age\":33,\"name\":\"John\"}"));
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

        assert_eq!(body.collect().await.unwrap().to_bytes(), Bytes::from_static(b"{\"age\":33,\"name\":\"John\"}"));
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

        assert_eq!(body.collect().await.unwrap().to_bytes(), Bytes::from_static(b"{\"age\":33,\"name\":\"John\"}"));
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
        let body = brotli(body, DecompressionLimits::default()
            .with_max_compressed(Limit::Limited(1))
            .resolved());

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
        let body = brotli(body, DecompressionLimits::default()
            .with_max_decompressed(Limit::Limited(1))
            .resolved());

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
        let body = brotli(body, DecompressionLimits::default()
            .with_max_expansion_ratio(ExpansionRatio::new(1, 0))
            .resolved());

        assert!(body.collect().await.is_err());
    }
    
    #[test]
    fn it_sets_decompression_limit_by_default() {
        let app = App::new();
        
        assert_eq!(app.decompression_limits.max_compressed_bytes, Limit::Limited(5 * 1024 * 1024));
        assert_eq!(app.decompression_limits.max_decompressed_bytes, Limit::Limited(16 * 1024 * 1024));
        assert_eq!(app.decompression_limits.max_expansion_ratio, Some(ExpansionRatio::new(100, 1024 * 1024)));
    }
}