//! Decompression middleware
//!
//! Middleware that decompress HTTP request body

#[cfg(feature = "decompression-brotli")]
use async_compression::tokio::bufread::BrotliDecoder;

#[cfg(feature = "decompression-gzip")]
use async_compression::tokio::bufread::{ZlibDecoder, GzipDecoder};

#[cfg(feature = "decompression-zstd")]
use async_compression::tokio::bufread::ZstdDecoder;

use futures_util::TryStreamExt;
use http_body_util::StreamBody;
use hyper::body::Frame;
use tokio_util::io::{
    ReaderStream,
    StreamReader
};

use crate::{
    App,
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
    http::request::request_body_limit::RequestBodyLimit,
    middleware::HttpContext, 
    HttpRequest, 
    HttpBody,
    status
};

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

macro_rules! impl_decompressor {
    ($algo:ident, $decoder:ident, $mm:literal) => {
        #[inline]
        fn $algo(body:  HttpBody) -> HttpBody {
            let body_stream = body.into_data_stream();
            let stream_reader = StreamReader::new(body_stream);
            let mut decoder = $decoder::new(stream_reader);
            decoder.multiple_members($mm);
            let decompressed_body = ReaderStream::new(decoder);
    
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
    /// Registers a middleware that applies a default decompression algorithm
    pub fn use_decompression(&mut self) -> &mut Self {
        self.wrap(|mut ctx, next| async move {
            if let Ok(content_encoding) = ctx.extract::<Header<ContentEncoding>>() {
                match content_encoding.into_inner().try_into() {
                    Ok(encoding) => {
                        let (req, handler) = ctx.into_parts();
                        let req = Self::decompress(encoding, req);
                        ctx = HttpContext::new(req, handler);
                    }
                    Err(error) if error.is_client_error() => (),
                    Err(_) => {
                        return status!(415, [
                            (VARY, CONTENT_ENCODING),
                            (ACCEPT_ENCODING, Encoding::stringify(SUPPORTED_ENCODINGS))
                        ]);
                    }
                }
            }
            next(ctx).await
        });
        self
    }

    fn decompress(encoding: Encoding, request: HttpRequest) -> HttpRequest {
        let (mut parts, body) = request.into_parts();
        
        parts.headers.remove(CONTENT_LENGTH);
        parts.headers.remove(CONTENT_ENCODING);
        
        let body = Self::decompress_body(encoding, body);
        let body_limit = parts.extensions.get::<RequestBodyLimit>()
            .cloned()
            .unwrap_or_default();
        
        HttpRequest::from_parts(parts, body)
            .into_limited(body_limit)
    }
    
    #[inline]
    fn decompress_body(encoding: Encoding, body: HttpBody) -> HttpBody {
        match encoding {
            #[cfg(feature = "decompression-brotli")]
            Encoding::Brotli => brotli(body),
            #[cfg(feature = "decompression-gzip")]
            Encoding::Gzip => gzip(body),
            #[cfg(feature = "decompression-gzip")]
            Encoding::Deflate => deflate(body),
            #[cfg(feature = "decompression-zstd")]
            Encoding::Zstd => zstd(body),
            _ => body
        }
    }
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
        let body = brotli(body);
        
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
        let body = gzip(body);

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
        let body = deflate(body);

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
        let body = zstd(body);

        assert_eq!(body.collect().await.unwrap().to_bytes(), Bytes::from_static(b"{\"age\":33,\"name\":\"John\"}"));
    }
}