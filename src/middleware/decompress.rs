﻿//! Decompression middleware
//!
//! Middleware that decompress HTTP request body

use std::io::{Error, ErrorKind};

#[cfg(feature = "decompression-brotli")]
use async_compression::tokio::bufread::BrotliDecoder;

#[cfg(feature = "decompression-gzip")]
use async_compression::tokio::bufread::{ZlibDecoder, GzipDecoder};

#[cfg(feature = "decompression-zstd")]
use async_compression::tokio::bufread::ZstdDecoder;

use futures_util::TryStreamExt;
use http_body_util::{BodyExt, StreamBody};
use hyper::body::Frame;
use tokio_util::io::{
    ReaderStream,
    StreamReader
};

use crate::{
    App, 
    headers::{
        ContentEncoding,
        Header,
        Encoding,
        CONTENT_ENCODING,
        CONTENT_LENGTH,
    }, 
    middleware::HttpContext, 
    HttpRequest, 
    HttpBody
};

macro_rules! impl_decompressor {
    ($algo:ident, $decoder:ident, $mm:literal) => {
        fn $algo(body:  HttpBody) -> HttpBody {
            let body_stream = body
                .map_err(|e| Error::new(ErrorKind::InvalidInput, e))
                .into_data_stream();
    
            let stream_reader = StreamReader::new(body_stream);
            let mut decoder = $decoder::new(stream_reader);
            decoder.multiple_members($mm);
            let decompressed_body = ReaderStream::new(decoder);
    
            HttpBody::boxed(
                StreamBody::new(decompressed_body
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
        self.use_middleware(|mut ctx, next| async move {
            if let Ok(content_encoding) = ctx.extract::<Header<ContentEncoding>>() {
                let (req, handler) = ctx.into_parts();
                let req = Self::decompress(content_encoding, req);
                ctx = HttpContext::new(req, handler);
            }
            next(ctx).await
        });
        self
    }

    #[cfg(feature = "di")]
    fn decompress(content_encoding: Header<ContentEncoding>, request: HttpRequest) -> HttpRequest {
        let encoding: Encoding = content_encoding.into_inner().into();
        let (mut parts, body, container) = request.into_parts();
        
        parts.headers.remove(CONTENT_LENGTH);
        parts.headers.remove(CONTENT_ENCODING);
        
        let body = Self::decompress_body(encoding, body);
        
        HttpRequest::from_parts(parts, body, container)
    }

    #[cfg(not(feature = "di"))]
    fn decompress(content_encoding: Header<ContentEncoding>, request: HttpRequest) -> HttpRequest {
        let encoding: Encoding = content_encoding.into_inner().into();
        let (mut parts, body) = request.into_parts();
        
        parts.headers.remove(CONTENT_LENGTH);
        parts.headers.remove(CONTENT_ENCODING);
        
        let body = Self::decompress_body(encoding, body);
        
        HttpRequest::from_parts(parts, body)
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
        
        let body = HttpBody::boxed(HttpBody::full(compressed));
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
            
        let body = HttpBody::boxed(HttpBody::full(compressed));
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

        let body = HttpBody::boxed(HttpBody::full(compressed));
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

        let body = HttpBody::boxed(HttpBody::full(compressed));
        let body = zstd(body);

        assert_eq!(body.collect().await.unwrap().to_bytes(), Bytes::from_static(b"{\"age\":33,\"name\":\"John\"}"));
    }
}