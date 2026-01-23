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

/// Represents current decompressions state
#[derive(Debug, Default)]
struct DecompressionState {
    compressed_bytes: AtomicUsize,
    decompressed_bytes: AtomicUsize,
}

#[inline]
fn decompression_error(kind: &str) -> Error {
    Error::from_parts(
        StatusCode::PAYLOAD_TOO_LARGE,
        None,
        format!("Decompression error: {kind}")
    )
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
                .map_err(Error::client_error)
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

fn limited_compressed_stream(
    body: HttpBody,
    limits: ResolvedDecompressionLimits,
    state: Arc<DecompressionState>
) -> impl TryStream<Ok = bytes::Bytes, Error = Error, Item = Result<bytes::Bytes, Error>> {
    body.into_data_stream()
        .map_err(|_| decompression_error("BodyReadFailed"))
        .and_then(move |chunk| {
            let new_total = state
                .compressed_bytes
                .fetch_add(chunk.len(), Ordering::Relaxed) + chunk.len();

            ready(if let Some(limit) = limits.max_compressed_bytes {
                if new_total > limit {
                    Err(decompression_error("CompressedBodyTooLarge"))
                } else {
                    Ok(chunk)
                }
            } else {
                Ok(chunk)
            })
        })
}

fn limited_decompressed_stream<R>(
    stream: R,
    limits: ResolvedDecompressionLimits,
    state: Arc<DecompressionState>
) -> impl TryStream<Ok = bytes::Bytes, Error = Error>
where
    R: TryStream<Ok = bytes::Bytes, Error = std::io::Error>,
{
    stream
        .map_err(|_| decompression_error("DecompressionFailed"))
        .and_then(move |chunk| {
            let new_total = state
                .decompressed_bytes
                .fetch_add(chunk.len(), Ordering::Relaxed) + chunk.len();

            let res = (|| {
                if limits.max_decompressed_bytes.is_some_and(|limit| new_total > limit) {
                    return Err(decompression_error("DecompressedBodyTooLarge"));
                }

                if let Some(ratio) = limits.max_expansion_ratio {
                    let compressed = state.compressed_bytes.load(Ordering::Relaxed);
                    let allowed = compressed
                        .saturating_mul(ratio.ratio)
                        .saturating_add(ratio.slack_bytes);

                    if new_total > allowed {
                        return Err(decompression_error("ExpansionRatioExceeded"));
                    }
                }

                Ok(chunk)
            })();

            ready(res)
        })
}

#[cfg(test)]
mod tests {
    use http_body_util::BodyExt;
    use bytes::Bytes;
    use tokio::io::AsyncWriteExt;
    use crate::HttpBody;
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
}