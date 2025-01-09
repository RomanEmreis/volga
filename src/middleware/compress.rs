﻿//! Compression middleware
//! 
//! Middleware that compress HTTP response body

use std::{
    collections::HashSet,
    cmp::Ordering,
    str::FromStr,
};

#[cfg(feature = "compression-brotli")]
use async_compression::tokio::bufread::BrotliEncoder;

#[cfg(feature = "compression-gzip")]
use async_compression::tokio::bufread::{ZlibEncoder, GzipEncoder};

#[cfg(feature = "compression-zstd")]
use async_compression::tokio::bufread::ZstdEncoder;

use async_compression::Level;
use futures_util::TryStreamExt;
use http_body_util::{BodyExt, StreamBody};
use hyper::body::Frame;
use hyper::http::response::Parts;
use tokio_util::io::{
    ReaderStream, 
    StreamReader
};

use crate::{
    App,
    headers::{
        AcceptEncoding,
        Header,
        Encoding,
        Quality,
        ACCEPT_ENCODING, ACCEPT_RANGES,
        CONTENT_ENCODING, CONTENT_LENGTH,
        VARY
    },
    HttpResponse,
    HttpResult,
    BoxBody,
    status
};

static SUPPORTED_ENCODINGS: &[Encoding] = &[
    Encoding::Identity,
    #[cfg(feature = "compression-brotli")]
    Encoding::Brotli,
    #[cfg(feature = "compression-gzip")]
    Encoding::Gzip,
    #[cfg(feature = "compression-gzip")]
    Encoding::Deflate,
    #[cfg(feature = "compression-zstd")]
    Encoding::Zstd,
];

macro_rules! impl_compressor {
    ($algo:ident, $encoder:ident, $level:expr) => {
        fn $algo(body: BoxBody) -> BoxBody {
            let stream_reader = StreamReader::new(body.into_data_stream());
            let encoder = $encoder::with_quality(stream_reader, $level);
            let compressed_body = ReaderStream::new(encoder);
            StreamBody::new(compressed_body.map_ok(Frame::data)).boxed()
        }
    };
}

#[cfg(feature = "compression-gzip")]
impl_compressor!(gzip, GzipEncoder, Level::Default);

#[cfg(feature = "compression-gzip")]
impl_compressor!(deflate, ZlibEncoder, Level::Default);

#[cfg(feature = "compression-brotli")]
impl_compressor!(brotli, BrotliEncoder, Level::Precise(4));

#[cfg(feature = "compression-zstd")]
impl_compressor!(zstd, ZstdEncoder, Level::Default);

impl App {
    /// Registers a middleware that applies a default compression algorithm
    pub fn use_compression(&mut self) -> &mut Self {
        self.use_middleware(|ctx, next| async move {
            let accept_encoding = ctx.extract::<Header<AcceptEncoding>>();
            let http_result = next(ctx).await;
            
            if let Ok(accept_encoding) = accept_encoding { 
                Self::negotiate(accept_encoding, http_result)
            } else { 
                http_result
            }  
        });
        self
    }
    
    fn negotiate(accept_encoding: Header<AcceptEncoding>, http_result: HttpResult) -> HttpResult {
        let accept_encoding = accept_encoding.into_inner();
        let mut encodings_with_weights = vec![];
        if let Ok(header_value) = accept_encoding.to_str() {
            for part in header_value.split(',') {
                let quality = Quality::<Encoding>::from_str(part.trim())?;
                encodings_with_weights.push(quality);
            }
            encodings_with_weights
                .sort_by(|a, b| b.value
                    .partial_cmp(&a.value)
                    .unwrap_or(Ordering::Equal)
                );
        }
        
        if encodings_with_weights.is_empty() { 
            return http_result;
        } 
        
        let supported = SUPPORTED_ENCODINGS
            .iter()
            .collect::<HashSet<_>>();
        
        if encodings_with_weights[0].item.is_any() {
            #[cfg(feature = "compression-brotli")]
            return Self::compress(Encoding::Brotli, http_result);
            
            #[cfg(all(
                feature = "compression-gzip", 
                not(feature = "compression-brotli"
            )))]
            return Self::compress(Encoding::Gzip, http_result);
            
            #[cfg(all(
                feature = "compression-zstd", 
                not(feature = "compression-brotli"), 
                not(feature = "compression-gzip"
            )))]
            return Self::compress(Encoding::Gzip, http_result);
            
            #[cfg(all(
                not(feature = "compression-brotli"), 
                not(feature = "compression-gzip"), 
                not(feature = "compression-zstd"), 
                not(feature = "compression-full"
            )))]
            return http_result;
        }

        for encoding in encodings_with_weights {
            if supported.contains(&encoding.item) { 
                return Self::compress(encoding.item, http_result);
            }
        }
        
        let supported_encodings_str = supported
            .iter()
            .map(|&encoding| encoding.to_string())
            .collect::<Vec<String>>()
            .join(",");
        
        status!(
            406, 
            supported_encodings_str, 
            [(VARY, ACCEPT_ENCODING)]
        )
    }

    fn compress(encoding: Encoding, http_result: HttpResult) -> HttpResult {
        if let Ok(response) = http_result {
            let (mut parts, body) = response.into_parts();
            parts.headers.remove(CONTENT_LENGTH);
            parts.headers.remove(ACCEPT_RANGES);
            parts.headers.append(VARY, ACCEPT_ENCODING.into());
            
            let body = Self::compress_body(&mut parts, encoding, body);
            
            Ok(HttpResponse::from_parts(parts, body))
        } else { 
            http_result
        }
    }

    fn compress_body(parts: &mut Parts, encoding: Encoding, body: BoxBody) -> BoxBody {
        match encoding {
            #[cfg(feature = "compression-brotli")]
            Encoding::Brotli => {
                parts.headers.append(CONTENT_ENCODING, Encoding::Brotli.into());
                brotli(body)
            },
            #[cfg(feature = "compression-gzip")]
            Encoding::Gzip => {
                parts.headers.append(CONTENT_ENCODING, Encoding::Gzip.into());
                gzip(body)
            },
            #[cfg(feature = "compression-gzip")]
            Encoding::Deflate => {
                parts.headers.append(CONTENT_ENCODING, Encoding::Deflate.into());
                deflate(body)
            },
            #[cfg(feature = "compression-zstd")]
            Encoding::Zstd => {
                parts.headers.append(CONTENT_ENCODING, Encoding::Zstd.into());
                zstd(body)
            },
            _ => body
        }
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;
    use tokio::io::AsyncWriteExt;
    use super::*;
    use crate::HttpBody;
    
    #[tokio::test]
    #[cfg(feature = "compression-brotli")]
    async fn in_compress_brotli() {
        use async_compression::tokio::write::BrotliDecoder;
        
        let body = HttpBody::json(json!({ "age": 33, "name": "John" }));
        let body = brotli(body);

        let mut decoder = BrotliDecoder::new(Vec::new());
        decoder.write_all(&body.collect().await.unwrap().to_bytes()).await.unwrap();
        decoder.shutdown().await.unwrap();
        let body = decoder.into_inner();
        
        assert_eq!(body, b"{\"age\":33,\"name\":\"John\"}".to_vec());
    }

    #[tokio::test]
    #[cfg(feature = "compression-gzip")]
    async fn in_compress_gzip() {
        use async_compression::tokio::write::GzipDecoder;

        let body = HttpBody::json(json!({ "age": 33, "name": "John" }));
        let body = gzip(body);

        let mut decoder = GzipDecoder::new(Vec::new());
        decoder.write_all(&body.collect().await.unwrap().to_bytes()).await.unwrap();
        decoder.shutdown().await.unwrap();
        let body = decoder.into_inner();

        assert_eq!(body, b"{\"age\":33,\"name\":\"John\"}".to_vec());
    }

    #[tokio::test]
    #[cfg(feature = "compression-gzip")]
    async fn in_compress_deflate() {
        use async_compression::tokio::write::ZlibDecoder;

        let body = HttpBody::json(json!({ "age": 33, "name": "John" }));
        let body = deflate(body);

        let mut decoder = ZlibDecoder::new(Vec::new());
        decoder.write_all(&body.collect().await.unwrap().to_bytes()).await.unwrap();
        decoder.shutdown().await.unwrap();
        let body = decoder.into_inner();

        assert_eq!(body, b"{\"age\":33,\"name\":\"John\"}".to_vec());
    }

    #[tokio::test]
    #[cfg(feature = "compression-zstd")]
    async fn in_compress_zstd() {
        use async_compression::tokio::write::ZstdDecoder;

        let body = HttpBody::json(json!({ "age": 33, "name": "John" }));
        let body = zstd(body);

        let mut decoder = ZstdDecoder::new(Vec::new());
        decoder.write_all(&body.collect().await.unwrap().to_bytes()).await.unwrap();
        decoder.shutdown().await.unwrap();
        let body = decoder.into_inner();

        assert_eq!(body, b"{\"age\":33,\"name\":\"John\"}".to_vec());
    }
}