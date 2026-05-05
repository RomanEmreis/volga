//! Multipart response encoder — boundary helpers and streaming body builder.

use crate::error::Error;
use rand::RngExt;
use std::sync::Arc;

const BOUNDARY_PREFIX: &str = "volga-";
const BOUNDARY_SUFFIX_LEN: usize = 32;
const BOUNDARY_ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

/// Generates a random RFC 2046 §5.1.1-compliant boundary in the form `volga-<32 alnum>`.
pub(super) fn generate_boundary() -> Arc<str> {
    let mut rng = rand::rng();
    let mut s = String::with_capacity(BOUNDARY_PREFIX.len() + BOUNDARY_SUFFIX_LEN);
    s.push_str(BOUNDARY_PREFIX);
    for _ in 0..BOUNDARY_SUFFIX_LEN {
        let i = rng.random_range(0..BOUNDARY_ALPHABET.len());
        s.push(BOUNDARY_ALPHABET[i] as char);
    }
    Arc::from(s)
}

/// Validates a boundary string per RFC 2046 §5.1.1:
/// - Length 1..=70
/// - Each char is `bcharsnospace` (alnum + `'()+_,-./:=?`) or space
/// - Last char is not a space
pub(super) fn validate_boundary(s: &str) -> Result<(), Error> {
    let bytes = s.as_bytes();
    if bytes.is_empty() || bytes.len() > 70 {
        return Err(Error::client_error(
            "multipart boundary must be between 1 and 70 characters",
        ));
    }
    for (i, &b) in bytes.iter().enumerate() {
        let is_last = i == bytes.len() - 1;
        let valid = if is_last {
            is_bcharsnospace(b)
        } else {
            is_bcharsnospace(b) || b == b' '
        };
        if !valid {
            return Err(Error::client_error(
                "multipart boundary contains invalid characters",
            ));
        }
    }
    Ok(())
}

#[inline]
fn is_bcharsnospace(b: u8) -> bool {
    matches!(
        b,
        b'0'..=b'9'
            | b'A'..=b'Z'
            | b'a'..=b'z'
            | b'\''
            | b'('
            | b')'
            | b'+'
            | b'_'
            | b','
            | b'-'
            | b'.'
            | b'/'
            | b':'
            | b'='
            | b'?'
    )
}

use crate::headers::{HeaderName, HeaderValue};
use crate::http::endpoints::args::multipart::Part;
use bytes::Bytes;

/// Writes `--<boundary>\r\n`.
#[inline]
pub(super) fn encode_boundary_open(boundary: &str) -> Bytes {
    Bytes::from(format!("--{boundary}\r\n"))
}

/// Writes `--<boundary>--\r\n`.
#[inline]
pub(super) fn encode_boundary_close(boundary: &str) -> Bytes {
    Bytes::from(format!("--{boundary}--\r\n"))
}

/// Encodes the per-part header block: Content-Type, Content-Disposition, then any extras.
/// Returns a single allocation containing all `Name: Value\r\n` lines, ready to be yielded.
#[inline]
pub(super) fn encode_part_headers(part: &Part) -> Bytes {
    let mut buf = Vec::with_capacity(64);
    if let Some(ct) = part.part_content_type() {
        write_header(&mut buf, &ct.name(), ct.value());
    }
    if let Some(cd) = part.part_content_disposition() {
        write_header(&mut buf, &cd.name(), cd.value());
    }
    if let Some(extras) = part.part_extras() {
        for (name, value) in extras.iter() {
            write_header(&mut buf, name, value);
        }
    }
    Bytes::from(buf)
}

fn write_header(out: &mut Vec<u8>, name: &HeaderName, value: &HeaderValue) {
    out.extend_from_slice(name.as_str().as_bytes());
    out.extend_from_slice(b": ");
    out.extend_from_slice(value.as_bytes());
    out.extend_from_slice(b"\r\n");
}

use crate::http::body::HttpBody;
use async_stream::try_stream;
use futures_util::stream::{BoxStream, StreamExt};

use super::part::PartBody;

/// Encodes a stream of [`Part`]s into a multipart [`HttpBody`].
///
/// The output stream yields one or more [`Bytes`] chunks per part:
/// boundary line, header block, separator CRLF, body (possibly chunked),
/// trailing CRLF; followed by the closing boundary at the end.
///
/// A `Result::Err` from the input stream (e.g. a parse failure during a
/// proxy/forward conversion) is propagated into the body stream so the
/// connection aborts mid-body instead of completing as a successful but
/// truncated multipart.
pub(super) fn encode(
    boundary: Arc<str>,
    mut parts: BoxStream<'static, Result<Part, Error>>,
) -> HttpBody {
    let stream = try_stream! {
        while let Some(part) = parts.next().await {
            let part = part?;
            yield encode_boundary_open(&boundary);
            yield encode_part_headers(&part);
            yield Bytes::from_static(b"\r\n");
            match part.into_body() {
                PartBody::Bytes(b) => {
                    if !b.is_empty() {
                        yield b;
                    }
                }
                PartBody::Stream(mut s) => {
                    while let Some(chunk) = s.next().await {
                        yield chunk?;
                    }
                }
            }
            yield Bytes::from_static(b"\r\n");
        }
        yield encode_boundary_close(&boundary);
    };
    // `try_stream!` yields `Result<Bytes, Error>`; help the compiler resolve the error type.
    let stream = stream.map(|r: Result<Bytes, Error>| r);
    HttpBody::stream(stream)
}

#[cfg(test)]
mod tests {
    use super::{generate_boundary, validate_boundary};

    #[test]
    fn validate_accepts_simple_token() {
        assert!(validate_boundary("X-BOUNDARY").is_ok());
        assert!(validate_boundary("a").is_ok());
        assert!(validate_boundary("0123456789").is_ok());
    }

    #[test]
    fn validate_accepts_max_length_70() {
        let s = "a".repeat(70);
        assert!(validate_boundary(&s).is_ok());
    }

    #[test]
    fn validate_rejects_empty() {
        assert!(validate_boundary("").is_err());
    }

    #[test]
    fn validate_rejects_over_70_chars() {
        let s = "a".repeat(71);
        assert!(validate_boundary(&s).is_err());
    }

    #[test]
    fn validate_rejects_invalid_char() {
        assert!(validate_boundary("bad\nboundary").is_err());
        assert!(validate_boundary("bad\tboundary").is_err());
        assert!(validate_boundary("with;semicolon").is_err());
    }

    #[test]
    fn validate_rejects_trailing_space() {
        assert!(validate_boundary("ends-with-space ").is_err());
    }

    #[test]
    fn validate_accepts_internal_space() {
        assert!(validate_boundary("has space inside").is_ok());
    }

    #[test]
    fn generate_format() {
        let b = generate_boundary();
        assert!(
            b.starts_with("volga-"),
            "boundary {b:?} should start with 'volga-'"
        );
        assert_eq!(b.len(), "volga-".len() + 32);
        assert!(validate_boundary(&b).is_ok());
    }

    #[test]
    fn generate_unique() {
        let a = generate_boundary();
        let b = generate_boundary();
        assert_ne!(a, b, "two generated boundaries should not collide");
    }

    use super::{encode_boundary_close, encode_boundary_open, encode_part_headers};
    use crate::headers::ContentType;
    use crate::http::endpoints::args::multipart::Part;

    #[test]
    fn boundary_open_format() {
        let b = encode_boundary_open("X");
        assert_eq!(&b[..], b"--X\r\n");
    }

    #[test]
    fn boundary_close_format() {
        let b = encode_boundary_close("X");
        assert_eq!(&b[..], b"--X--\r\n");
    }

    #[test]
    fn part_headers_ct_then_cd() {
        let p = Part::text("name", "v");
        let bytes = encode_part_headers(&p);
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(
            s.starts_with("content-type: text/plain; charset=utf-8\r\n"),
            "headers: {s:?}"
        );
        assert!(s.contains("content-disposition: form-data; name=\"name\"\r\n"));
    }

    #[test]
    fn part_headers_extras_appear_after_ct_cd() {
        let p = Part::text("n", "v")
            .with_content_type(ContentType::text_utf_8())
            .with_header_raw(
                crate::headers::HeaderName::from_static("x-custom"),
                crate::headers::HeaderValue::from_static("y"),
            );
        let bytes = encode_part_headers(&p);
        let s = std::str::from_utf8(&bytes).unwrap();
        let ct_pos = s.find("content-type:").expect("ct");
        let cd_pos = s.find("content-disposition:").expect("cd");
        let xc_pos = s.find("x-custom:").expect("x-custom");
        assert!(ct_pos < xc_pos);
        assert!(cd_pos < xc_pos);
    }

    #[test]
    fn part_headers_no_extras_when_extra_is_none() {
        let p = Part::text("n", "v");
        let bytes = encode_part_headers(&p);
        let s = std::str::from_utf8(&bytes).unwrap();
        assert!(!s.contains("x-"));
    }

    #[test]
    fn part_headers_preserve_raw_obs_text_bytes() {
        // RFC 7230 obs-text (0x80..=0xFF) is valid in HeaderValue but not UTF-8.
        // The encoder must emit the original bytes, not substitute a placeholder.
        let raw = crate::headers::HeaderValue::from_bytes(b"\xC3\x28").unwrap(); // invalid UTF-8
        let p = Part::text("n", "v")
            .with_header_raw(crate::headers::HeaderName::from_static("x-binary"), raw);
        let bytes = encode_part_headers(&p);
        let needle = b"x-binary: \xC3(\r\n";
        assert!(
            bytes.windows(needle.len()).any(|w| w == needle),
            "expected raw obs-text bytes to be preserved verbatim, got {bytes:?}"
        );
    }

    use super::encode;
    use bytes::Bytes;
    use futures_util::stream;
    use http_body_util::BodyExt;
    use std::sync::Arc;

    async fn drain(body: crate::http::body::HttpBody) -> Vec<u8> {
        let collected = body.collect().await.unwrap();
        collected.to_bytes().to_vec()
    }

    #[tokio::test]
    async fn encode_empty_parts_produces_only_closing_boundary() {
        let boundary: Arc<str> = Arc::from("X-BOUNDARY");
        let parts = Box::pin(stream::iter(Vec::<Result<Part, crate::error::Error>>::new())) as _;
        let body = encode(boundary, parts);
        let bytes = drain(body).await;
        assert_eq!(bytes, b"--X-BOUNDARY--\r\n");
    }

    #[tokio::test]
    async fn encode_single_text_part_exact_bytes() {
        let boundary: Arc<str> = Arc::from("X-BOUNDARY");
        let parts = Box::pin(stream::iter(vec![Ok::<_, crate::error::Error>(
            Part::text("name", "abcd"),
        )])) as _;
        let body = encode(boundary, parts);
        let s = String::from_utf8(drain(body).await).unwrap();
        assert!(s.contains("--X-BOUNDARY\r\n"));
        assert!(s.contains("content-type: text/plain; charset=utf-8\r\n"));
        assert!(s.contains("content-disposition: form-data; name=\"name\"\r\n"));
        assert!(s.contains("\r\n\r\nabcd\r\n"));
        assert!(s.ends_with("--X-BOUNDARY--\r\n"));
    }

    #[tokio::test]
    async fn encode_round_trips_through_multer() {
        let boundary: Arc<str> = Arc::from("ROUND-TRIP");
        let parts = Box::pin(stream::iter(vec![
            Ok::<_, crate::error::Error>(Part::text("name1", "value1")),
            Ok(Part::file(
                "upload",
                "data.bin",
                Bytes::from_static(b"\x01\x02\x03"),
            )),
        ])) as _;
        let body = encode(boundary.clone(), parts);
        let bytes = drain(body).await;

        let mut mp = multer::Multipart::new(
            stream::iter(vec![Ok::<_, std::io::Error>(Bytes::from(bytes))]),
            boundary.as_ref(),
        );
        let f1 = mp.next_field().await.unwrap().unwrap();
        assert_eq!(f1.name(), Some("name1"));
        assert_eq!(f1.text().await.unwrap(), "value1");

        let f2 = mp.next_field().await.unwrap().unwrap();
        assert_eq!(f2.name(), Some("upload"));
        assert_eq!(f2.file_name(), Some("data.bin"));
        assert_eq!(
            f2.bytes().await.unwrap(),
            Bytes::from_static(b"\x01\x02\x03")
        );

        assert!(mp.next_field().await.unwrap().is_none());
    }

    #[tokio::test]
    async fn encode_streaming_body_drains_chunks() {
        let boundary: Arc<str> = Arc::from("STREAM");
        let chunks = stream::iter(vec![
            Ok::<_, crate::error::Error>(Bytes::from_static(b"chunk-1-")),
            Ok(Bytes::from_static(b"chunk-2")),
        ]);
        let part = Part::stream(
            "log",
            "log.txt",
            crate::headers::ContentType::text_utf_8(),
            chunks,
        );
        let parts = Box::pin(stream::iter(vec![Ok::<_, crate::error::Error>(part)])) as _;
        let body = encode(boundary, parts);
        let s = String::from_utf8(drain(body).await).unwrap();
        assert!(s.contains("\r\n\r\nchunk-1-chunk-2\r\n"), "got: {s}");
    }

    #[tokio::test]
    async fn encode_propagates_input_error() {
        use http_body_util::BodyExt;
        let boundary: Arc<str> = Arc::from("ERR-BDY");
        let parts = Box::pin(stream::iter(vec![
            Ok(Part::text("ok", "first")),
            Err(crate::error::Error::client_error("simulated parse failure")),
        ])) as _;
        let body = encode(boundary, parts);
        let err = body.collect().await.unwrap_err();
        assert!(format!("{err}").contains("simulated parse failure"));
    }
}
