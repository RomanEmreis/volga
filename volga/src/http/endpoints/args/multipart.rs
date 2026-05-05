//! Extractors and response types for multipart data.

pub use field::Field;
pub use part::{Part, PartBody};

use crate::error::Error;
use crate::headers::{CONTENT_TYPE, HeaderMap};
use error::MultipartError;
use futures_util::future::{Ready, ready};

use std::{borrow::Cow, path::Path, sync::Arc};

use crate::http::endpoints::args::{FromPayload, Payload, Source};

mod encoder;
mod error;
mod field;
mod part;

/// Multipart content — extractor on the request side, response on the outgoing side.
///
/// # Inbound (extractor)
/// ```no_run
/// use volga::{HttpResult, Multipart, ok};
///
/// async fn handle(multipart: Multipart) -> HttpResult {
///     multipart.save_all("path/to/folder").await?;
///     ok!("Files saved!")
/// }
/// ```
///
/// # Outbound (response)
/// ```no_run
/// use volga::Multipart;
/// use volga::multipart::Part;
///
/// async fn handle() -> Multipart {
///     Multipart::from_parts(vec![
///         Part::text("greeting", "hi"),
///         Part::file("logo", "logo.png", b"...image bytes...".to_vec()),
///     ])
/// }
/// # let _ = handle;
/// ```
pub struct Multipart {
    inner: MultipartInner,
}

impl std::fmt::Debug for Multipart {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.inner {
            MultipartInner::Incoming { boundary, .. } => f
                .debug_struct("Multipart::Incoming")
                .field("boundary", boundary)
                .finish_non_exhaustive(),
            MultipartInner::Outgoing {
                subtype, boundary, ..
            } => f
                .debug_struct("Multipart::Outgoing")
                .field("subtype", subtype)
                .field("boundary", boundary)
                .finish_non_exhaustive(),
        }
    }
}

// `Incoming.boundary` is a `String` because `multer::Multipart` requires `impl Into<String>`
// at construction; `Outgoing.boundary` is `Arc<str>` so it can be cheaply shared with the
// streaming encoder. Asymmetry is intentional.
pub(crate) enum MultipartInner {
    Incoming {
        subtype: MultipartSubtype,
        boundary: String,
        multipart: multer::Multipart<'static>,
    },
    Outgoing {
        subtype: MultipartSubtype,
        boundary: Arc<str>,
        #[allow(dead_code)]
        parts: futures_util::stream::BoxStream<'static, Result<Part, Error>>,
    },
}

/// RFC 2046 multipart subtype. Defaults to `FormData` for outbound.
#[derive(Debug, Clone)]
pub enum MultipartSubtype {
    /// `multipart/form-data` — the canonical form / file upload subtype.
    FormData,
    /// `multipart/mixed` — heterogeneous parts.
    Mixed,
    /// `multipart/byteranges` — partial-content responses for HTTP `Range` requests.
    ByteRanges,
    /// Any other subtype, e.g. `alternative`, `related`.
    Custom(Cow<'static, str>),
}

impl MultipartSubtype {
    #[allow(dead_code)]
    pub(crate) fn as_str(&self) -> &str {
        match self {
            Self::FormData => "form-data",
            Self::Mixed => "mixed",
            Self::ByteRanges => "byteranges",
            Self::Custom(s) => s.as_ref(),
        }
    }

    /// Parses the subtype from a `multipart/<subtype>[; ...]` Content-Type header value.
    /// Falls back to [`Self::FormData`] if the value is malformed (caller has already
    /// confirmed a boundary exists, so the value is structurally a multipart Content-Type).
    fn from_content_type(value: &str) -> Self {
        let after_slash = value.split_once('/').map(|(_, rest)| rest).unwrap_or("");
        let token = after_slash.split(';').next().unwrap_or("").trim();
        match token {
            "" | "form-data" => Self::FormData,
            "mixed" => Self::Mixed,
            "byteranges" => Self::ByteRanges,
            other => Self::Custom(Cow::Owned(other.to_owned())),
        }
    }
}

impl Multipart {
    /// Asynchronously writes incoming multipart files to disk.
    /// Errors if called on an outgoing multipart.
    pub async fn save_all(mut self, path: impl AsRef<Path>) -> Result<(), Error> {
        while let Some(field) = self.next_field().await? {
            field.save(&path).await?;
        }
        Ok(())
    }

    /// Yields the next [`Field`] from an incoming multipart, or `None` when exhausted.
    /// Errors with a descriptive message if called on an outgoing multipart.
    #[inline]
    pub async fn next_field(&mut self) -> Result<Option<Field>, Error> {
        match &mut self.inner {
            MultipartInner::Incoming { multipart, .. } => multipart
                .next_field()
                .await
                .map_err(MultipartError::read_error)
                .map(|f| f.map(Field)),
            MultipartInner::Outgoing { .. } => Err(Error::server_error(
                "next_field called on an outgoing multipart",
            )),
        }
    }

    /// Returns the boundary string of this multipart.
    #[inline]
    pub fn boundary(&self) -> &str {
        match &self.inner {
            MultipartInner::Incoming { boundary, .. } => boundary,
            MultipartInner::Outgoing { boundary, .. } => boundary,
        }
    }

    /// Extracts the `boundary` parameter from a `multipart/*` Content-Type header.
    /// Subtype-agnostic — accepts any `multipart/<subtype>`, not just form-data —
    /// because volga supports forwarding `byteranges`, `mixed`, etc.
    ///
    /// Walks parameters as `(name, value)` pairs (RFC 7231 §3.1.1.1) so a quoted
    /// value containing the substring `boundary=` (e.g. `foo="xboundary=y"`) does
    /// not confuse the match — the parameter name lookup is structural, not textual.
    fn parse_boundary(headers: &HeaderMap) -> Option<String> {
        let content_type = headers.get(CONTENT_TYPE)?.to_str().ok()?;
        let trimmed = content_type.trim_start();
        let (ty, after_slash) = trimmed.split_once('/')?;
        if !ty.eq_ignore_ascii_case("multipart") {
            return None;
        }
        // Skip the subtype to the first ';'; absent ';' means no parameters at all.
        let mut rest = after_slash.split_once(';')?.1;
        loop {
            rest = trim_ows(rest);
            if rest.is_empty() {
                return None;
            }
            let (name, after_eq) = rest.split_once('=')?;
            let name = name.trim();
            let (value, tail) = parse_param_value(trim_ows(after_eq))?;
            if name.eq_ignore_ascii_case("boundary") {
                return if value.is_empty() { None } else { Some(value) };
            }
            // Advance past the trailing ';' (if any) to the next parameter.
            rest = trim_ows(tail).strip_prefix(';')?;
        }
    }

    #[inline]
    fn parse_subtype(headers: &HeaderMap) -> MultipartSubtype {
        headers
            .get(CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .map(MultipartSubtype::from_content_type)
            .unwrap_or(MultipartSubtype::FormData)
    }

    /// Consumes self and returns the inner enum.
    #[allow(dead_code)]
    pub(crate) fn into_inner(self) -> MultipartInner {
        self.inner
    }

    /// Builds an outgoing form-data multipart from any iterator of [`Part`]s.
    /// Default subtype is [`MultipartSubtype::FormData`]; override via [`Multipart::with_subtype`].
    /// Boundary is auto-generated.
    pub fn from_parts<I>(parts: I) -> Self
    where
        I: IntoIterator<Item = Part>,
        I::IntoIter: Send + 'static,
    {
        Self::from_stream(futures_util::stream::iter(parts))
    }

    /// Builds an outgoing multipart from a streaming source of [`Part`]s — useful when
    /// parts are produced lazily (e.g. enumerating files, computing byte ranges).
    pub fn from_stream<S>(parts: S) -> Self
    where
        S: futures_util::Stream<Item = Part> + Send + 'static,
    {
        use futures_util::StreamExt;
        Self {
            inner: MultipartInner::Outgoing {
                subtype: MultipartSubtype::FormData,
                boundary: encoder::generate_boundary(),
                parts: parts.map(Ok).boxed(),
            },
        }
    }

    /// Overrides the multipart subtype (defaults to [`MultipartSubtype::FormData`]).
    /// No-op on incoming multiparts.
    pub fn with_subtype(mut self, new_subtype: MultipartSubtype) -> Self {
        if let MultipartInner::Outgoing {
            ref mut subtype, ..
        } = self.inner
        {
            *subtype = new_subtype;
        }
        self
    }

    /// Returns the canonical Content-Type header for this outgoing multipart.
    /// Returns `Err` for incoming multiparts (with a server-side error explaining the misuse)
    /// or when the subtype contains bytes invalid in an HTTP header value.
    pub(crate) fn content_type_header(
        &self,
    ) -> Result<crate::headers::Header<crate::headers::ContentType>, Error> {
        let MultipartInner::Outgoing {
            subtype, boundary, ..
        } = &self.inner
        else {
            return Err(Error::server_error(
                "cannot return incoming multipart as response; call into_outgoing() first",
            ));
        };
        use crate::headers::ContentType;
        Ok(match subtype {
            MultipartSubtype::FormData => ContentType::multipart_form_data(boundary),
            MultipartSubtype::Mixed => ContentType::multipart_mixed(boundary),
            MultipartSubtype::ByteRanges => ContentType::multipart_byte_ranges(boundary),
            MultipartSubtype::Custom(s) => ContentType::multipart_custom(s, boundary)?,
        })
    }

    /// Overrides the auto-generated boundary. Validates per RFC 2046 §5.1.1.
    /// Errors if the boundary is malformed; no-op on incoming multiparts.
    pub fn with_boundary(mut self, new_boundary: impl Into<Arc<str>>) -> Result<Self, Error> {
        let new_boundary = new_boundary.into();
        encoder::validate_boundary(&new_boundary)?;
        if let MultipartInner::Outgoing {
            ref mut boundary, ..
        } = self.inner
        {
            *boundary = new_boundary;
        }
        Ok(self)
    }

    /// Re-encodes an incoming multipart as an outgoing one, lazily streaming each
    /// field as a [`Part`] with a streaming body. Useful for proxy / forwarding.
    /// **Not byte-perfect** (boundary regenerates, header ordering may differ).
    /// For byte-perfect passthrough, skip the [`Multipart`] extractor and forward
    /// the raw [`HttpBody`](crate::http::body::HttpBody).
    ///
    /// Errors if called on an already-outgoing multipart.
    pub fn into_outgoing(self) -> Result<Self, Error> {
        let MultipartInner::Incoming {
            subtype,
            boundary,
            mut multipart,
        } = self.inner
        else {
            return Err(Error::server_error(
                "into_outgoing called on a multipart that is already outgoing",
            ));
        };

        let boundary: Arc<str> = Arc::from(boundary);
        let parts_stream = async_stream::try_stream! {
            while let Some(field) = multipart
                .next_field()
                .await
                .map_err(MultipartError::read_error)?
            {
                yield field_to_part(field);
            }
        };

        Ok(Self {
            inner: MultipartInner::Outgoing {
                subtype,
                boundary,
                parts: Box::pin(parts_stream),
            },
        })
    }
}

impl From<Vec<Part>> for Multipart {
    #[inline]
    fn from(parts: Vec<Part>) -> Self {
        Self::from_parts(parts)
    }
}

impl<'a> TryFrom<Payload<'a>> for Multipart {
    type Error = Error;

    #[inline]
    fn try_from(payload: Payload<'a>) -> Result<Self, Self::Error> {
        let Payload::Full(parts, body) = payload else {
            unreachable!("Multipart requires Payload::Full; SOURCE = Source::Full enforces this");
        };
        let boundary =
            Self::parse_boundary(&parts.headers).ok_or(MultipartError::invalid_boundary())?;
        let subtype = Self::parse_subtype(&parts.headers);
        let stream = body.into_data_stream();
        let multipart = multer::Multipart::new(stream, boundary.clone());
        Ok(Multipart {
            inner: MultipartInner::Incoming {
                subtype,
                boundary,
                multipart,
            },
        })
    }
}

/// Extracts a file stream from the request body
impl FromPayload for Multipart {
    type Future = Ready<Result<Self, Error>>;
    const SOURCE: Source = Source::Full;

    #[inline]
    fn from_payload(payload: Payload<'_>) -> Self::Future {
        ready(payload.try_into())
    }

    #[cfg(feature = "openapi")]
    fn describe_openapi(
        config: crate::openapi::OpenApiRouteConfig,
    ) -> crate::openapi::OpenApiRouteConfig {
        config.consumes_multipart()
    }
}

/// Converts a single [`multer::Field`] into a [`Part`] whose body is a stream that
/// drains chunks lazily from the field. No buffering.
///
/// Forwards every per-part header verbatim — `Content-Type`, `Content-Disposition`
/// (preserving `filename*` and other parameters), `Content-Range`, plus any custom
/// header — so proxy / forwarding flows produce a semantically-equivalent body.
/// Strips RFC 7230 OWS (optional whitespace: SP / HTAB) from the front of `s`.
#[inline]
fn trim_ows(s: &str) -> &str {
    s.trim_start_matches([' ', '\t'])
}

/// Parses a Content-Type parameter value at the start of `s`. Handles either a
/// `token` (consumed up to the next `;` or end) or a `quoted-string` per RFC 7230
/// (with backslash-quoted-pair escapes). Returns the unquoted value and the
/// remainder of the input after the value.
fn parse_param_value(s: &str) -> Option<(String, &str)> {
    if let Some(after_quote) = s.strip_prefix('"') {
        let mut value = String::new();
        let mut chars = after_quote.char_indices();
        while let Some((idx, c)) = chars.next() {
            match c {
                '"' => return Some((value, &after_quote[idx + c.len_utf8()..])),
                '\\' => match chars.next() {
                    Some((_, esc)) => value.push(esc),
                    None => return None,
                },
                other => value.push(other),
            }
        }
        None
    } else {
        let end = s.find(';').unwrap_or(s.len());
        let value = s[..end].trim_end_matches([' ', '\t']).to_owned();
        Some((value, &s[end..]))
    }
}

fn field_to_part(mut field: multer::Field<'static>) -> Part {
    use crate::headers::{ContentDisposition, ContentType, Header};

    // Snapshot headers before `field.chunk()` takes a mutable borrow.
    let headers = field.headers().clone();

    let body_stream = async_stream::try_stream! {
        while let Some(chunk) = field
            .chunk()
            .await
            .map_err(|e| Error::client_error(format!("multipart read: {e}")))?
        {
            yield chunk;
        }
    };
    let mut part = Part::new(PartBody::Stream(Box::pin(body_stream)));

    for (name, value) in headers.iter() {
        if name == CONTENT_TYPE {
            if let Ok(ct) = Header::<ContentType>::from_bytes(value.as_bytes()) {
                part = part.with_content_type(ct);
            }
        } else if name == crate::headers::CONTENT_DISPOSITION {
            if let Ok(cd) = Header::<ContentDisposition>::from_bytes(value.as_bytes()) {
                part = part.with_disposition_raw(cd);
            }
        } else {
            part = part.with_header_raw(name.clone(), value.clone());
        }
    }
    part
}

/// Encodes an outgoing parts stream into an HTTP body. Wraps `encoder::encode`
/// so the encoder module stays `pub(super)`.
pub(crate) fn encode_for_response(
    boundary: Arc<str>,
    parts: futures_util::stream::BoxStream<'static, Result<Part, Error>>,
) -> crate::http::body::HttpBody {
    encoder::encode(boundary, parts)
}

#[cfg(test)]
mod tests {
    use super::Multipart;
    use crate::headers::CONTENT_TYPE;
    use crate::http::body::HttpBody;
    use crate::http::endpoints::args::{FromPayload, Payload};
    use hyper::Request;

    fn make_headers(content_type: &str) -> crate::headers::HeaderMap {
        let mut h = crate::headers::HeaderMap::new();
        h.insert(
            CONTENT_TYPE,
            crate::headers::HeaderValue::from_str(content_type).unwrap(),
        );
        h
    }

    #[test]
    fn parse_boundary_simple_token() {
        let h = make_headers("multipart/form-data; boundary=ABCDEF");
        assert_eq!(Multipart::parse_boundary(&h).as_deref(), Some("ABCDEF"));
    }

    #[test]
    fn parse_boundary_quoted_value() {
        let h = make_headers(r#"multipart/form-data; boundary="with space""#);
        assert_eq!(Multipart::parse_boundary(&h).as_deref(), Some("with space"));
    }

    #[test]
    fn parse_boundary_skips_other_quoted_param_containing_boundary_substring() {
        // Regression: substring scan would have matched the literal "boundary=" inside
        // foo's quoted value. The structured parser must skip it and pick the real one.
        let h = make_headers(r#"multipart/form-data; foo="xboundary=y"; boundary=real"#);
        assert_eq!(Multipart::parse_boundary(&h).as_deref(), Some("real"));
    }

    #[test]
    fn parse_boundary_case_insensitive_type_and_param_name() {
        let h = make_headers("MULTIPART/Form-Data; BOUNDARY=ZZZ");
        assert_eq!(Multipart::parse_boundary(&h).as_deref(), Some("ZZZ"));
    }

    #[test]
    fn parse_boundary_rejects_non_multipart_type() {
        let h = make_headers("text/plain; boundary=ZZZ");
        assert_eq!(Multipart::parse_boundary(&h), None);
    }

    #[test]
    fn parse_boundary_rejects_when_param_absent() {
        let h = make_headers("multipart/form-data; charset=utf-8");
        assert_eq!(Multipart::parse_boundary(&h), None);
    }

    #[tokio::test]
    async fn it_reads_from_payload() {
        let req = create_multipart_req();
        let (parts, body) = req.into_parts();
        let mut multipart = Multipart::from_payload(Payload::Full(&parts, body))
            .await
            .unwrap();

        while let Some(field) = multipart.next_field().await.unwrap() {
            assert_eq!(field.name().unwrap(), "my_text_field");
            assert_eq!(field.text().await.unwrap(), "abcd");
        }
    }

    #[tokio::test]
    async fn it_reads_file_name() {
        let req = create_multipart_req();
        let (parts, body) = req.into_parts();
        let mut multipart = Multipart::from_payload(Payload::Full(&parts, body))
            .await
            .unwrap();

        while let Some(field) = multipart.next_field().await.unwrap() {
            assert_eq!(field.try_get_file_name().unwrap(), "my_text_field");
        }
    }

    fn create_multipart_req() -> Request<HttpBody> {
        let data = "--X-BOUNDARY\r\nContent-Disposition: form-data; name=\"my_text_field\"\r\n\r\nabcd\r\n--X-BOUNDARY--\r\n";

        Request::get("/")
            .header(CONTENT_TYPE, "multipart/form-data; boundary=X-BOUNDARY")
            .body(HttpBody::full(data))
            .unwrap()
    }

    use super::{MultipartInner, MultipartSubtype, Part};

    #[tokio::test]
    async fn from_parts_vec() {
        let mp = Multipart::from_parts(vec![Part::text("a", "1"), Part::text("b", "2")]);
        assert!(matches!(mp.inner, MultipartInner::Outgoing { .. }));
        assert!(mp.boundary().starts_with("volga-"));
    }

    #[tokio::test]
    async fn from_parts_array() {
        let _mp = Multipart::from_parts([Part::text("a", "1"), Part::text("b", "2")]);
    }

    #[tokio::test]
    async fn from_vec_via_into() {
        let mp: Multipart = vec![Part::text("a", "1")].into();
        assert!(matches!(mp.inner, MultipartInner::Outgoing { .. }));
    }

    #[tokio::test]
    async fn from_stream_works() {
        use futures_util::stream;
        let mp = Multipart::from_stream(stream::iter(vec![Part::text("a", "1")]));
        assert!(matches!(mp.inner, MultipartInner::Outgoing { .. }));
    }

    #[tokio::test]
    async fn with_subtype_changes_subtype() {
        let mp = Multipart::from_parts(vec![Part::text("a", "1")])
            .with_subtype(MultipartSubtype::ByteRanges);
        if let MultipartInner::Outgoing { subtype, .. } = mp.inner {
            assert!(matches!(subtype, MultipartSubtype::ByteRanges));
        } else {
            panic!("expected Outgoing");
        }
    }

    #[tokio::test]
    async fn with_boundary_validates() {
        let mp = Multipart::from_parts(vec![Part::text("a", "1")]);
        assert!(mp.with_boundary("good-boundary").is_ok());

        let mp = Multipart::from_parts(vec![Part::text("a", "1")]);
        assert!(mp.with_boundary("bad\nboundary").is_err());
    }

    #[tokio::test]
    async fn with_boundary_overrides() {
        let mp = Multipart::from_parts(vec![Part::text("a", "1")])
            .with_boundary("custom")
            .unwrap();
        assert_eq!(mp.boundary(), "custom");
    }

    #[tokio::test]
    async fn next_field_on_outgoing_returns_error() {
        let mut mp = Multipart::from_parts(vec![Part::text("a", "1")]);
        let err = mp.next_field().await.unwrap_err();
        assert!(format!("{err}").contains("outgoing"));
    }

    #[tokio::test]
    async fn into_response_outgoing_yields_correct_content_type() {
        use crate::http::IntoResponse;
        let mp = Multipart::from_parts(vec![Part::text("a", "1")])
            .with_boundary("X-BDY")
            .unwrap();
        let resp = mp.into_response().unwrap();
        let ct = resp
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(ct, "multipart/form-data; boundary=X-BDY");
    }

    #[tokio::test]
    async fn into_response_byteranges_subtype() {
        use crate::http::IntoResponse;
        let mp = Multipart::from_parts(vec![Part::new(b"abc" as &[u8])])
            .with_subtype(MultipartSubtype::ByteRanges)
            .with_boundary("R")
            .unwrap();
        let resp = mp.into_response().unwrap();
        let ct = resp
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap();
        assert_eq!(ct, "multipart/byteranges; boundary=R");
    }

    #[tokio::test]
    async fn into_response_incoming_returns_error() {
        use crate::http::IntoResponse;
        // Build an incoming Multipart through the extractor path
        let req = create_multipart_req();
        let (parts, body) = req.into_parts();
        let mp = Multipart::from_payload(Payload::Full(&parts, body))
            .await
            .unwrap();
        let err = mp.into_response().unwrap_err();
        assert!(format!("{err}").contains("incoming"));
    }

    #[tokio::test]
    async fn into_outgoing_round_trips_through_multer() {
        use crate::http::IntoResponse;
        use http_body_util::BodyExt;

        // 1. Build an incoming multipart from a known wire string
        let req = create_multipart_req();
        let (parts, body) = req.into_parts();
        let mp = Multipart::from_payload(Payload::Full(&parts, body))
            .await
            .unwrap();

        // 2. Convert to outgoing and encode
        let out = mp.into_outgoing().unwrap();
        let resp = out.into_response().unwrap();

        // 3. Read out the encoded body
        let ct = resp
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let bytes = resp
            .into_inner()
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();

        // 4. Re-parse it with multer and assert the original field is preserved
        let boundary = multer::parse_boundary(&ct).unwrap();
        let mut mp2 = multer::Multipart::new(
            futures_util::stream::iter(vec![Ok::<_, std::io::Error>(bytes)]),
            boundary,
        );
        let f = mp2.next_field().await.unwrap().unwrap();
        assert_eq!(f.name(), Some("my_text_field"));
        assert_eq!(f.text().await.unwrap(), "abcd");
    }

    #[tokio::test]
    async fn into_outgoing_on_already_outgoing_returns_error() {
        let mp = Multipart::from_parts(vec![Part::text("a", "1")]);
        let err = mp.into_outgoing().unwrap_err();
        assert!(format!("{err}").contains("already"));
    }

    #[tokio::test]
    async fn into_outgoing_preserves_part_content_type_parameters() {
        use crate::http::IntoResponse;
        use http_body_util::BodyExt;

        // An inbound part declares a Content-Type with a `charset` parameter.
        let body = "--B\r\nContent-Disposition: form-data; name=\"f\"\r\nContent-Type: text/plain; charset=us-ascii\r\n\r\nhello\r\n--B--\r\n";
        let req = Request::get("/")
            .header(CONTENT_TYPE, "multipart/form-data; boundary=B")
            .body(HttpBody::full(body))
            .unwrap();
        let (parts, body) = req.into_parts();
        let mp = Multipart::from_payload(Payload::Full(&parts, body))
            .await
            .unwrap();

        let resp = mp.into_outgoing().unwrap().into_response().unwrap();
        let ct = resp
            .headers()
            .get("content-type")
            .unwrap()
            .to_str()
            .unwrap()
            .to_owned();
        let bytes = resp
            .into_inner()
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let wire = std::str::from_utf8(&bytes).unwrap();
        assert!(
            wire.contains("content-type: text/plain; charset=us-ascii"),
            "expected charset parameter to survive forwarding; got: {wire}\nresponse CT: {ct}"
        );
    }

    #[tokio::test]
    async fn into_outgoing_preserves_incoming_subtype() {
        // Inbound is multipart/byteranges; into_outgoing must keep that subtype on the
        // response Content-Type instead of rewriting to multipart/form-data.
        let body = "--BNDRY\r\nContent-Range: bytes 0-4/10\r\nContent-Type: text/plain\r\n\r\nfirst\r\n--BNDRY--\r\n";
        let req = Request::get("/")
            .header(CONTENT_TYPE, "multipart/byteranges; boundary=BNDRY")
            .body(HttpBody::full(body))
            .unwrap();
        let (parts, body) = req.into_parts();
        let mp = Multipart::from_payload(Payload::Full(&parts, body))
            .await
            .unwrap();

        let outgoing = mp.into_outgoing().unwrap();
        let ct = outgoing.content_type_header().unwrap();
        let ct_str = ct.as_ref().to_str().unwrap();
        assert!(
            ct_str.starts_with("multipart/byteranges"),
            "expected byteranges to survive forwarding, got: {ct_str}"
        );
    }

    #[tokio::test]
    async fn into_outgoing_forwards_per_part_headers() {
        use crate::http::IntoResponse;
        use http_body_util::BodyExt;

        // Source part has Content-Range, a filename* parameter on Content-Disposition,
        // and a custom header — none of which the form-data builder API would set.
        // All must survive the proxy round-trip.
        let body = "--BNDRY\r\n\
            Content-Disposition: form-data; name=\"upload\"; filename=\"plain.txt\"; filename*=UTF-8''r%C3%A9sum%C3%A9.txt\r\n\
            Content-Type: text/plain; charset=utf-8\r\n\
            Content-Range: bytes 0-4/10\r\n\
            X-Custom-Trace: trace-abc\r\n\
            \r\n\
            hello\r\n--BNDRY--\r\n";
        let req = Request::get("/")
            .header(CONTENT_TYPE, "multipart/form-data; boundary=BNDRY")
            .body(HttpBody::full(body))
            .unwrap();
        let (parts, body) = req.into_parts();
        let mp = Multipart::from_payload(Payload::Full(&parts, body))
            .await
            .unwrap();

        let resp = mp.into_outgoing().unwrap().into_response().unwrap();
        let bytes = resp
            .into_inner()
            .into_body()
            .collect()
            .await
            .unwrap()
            .to_bytes();
        let wire = std::str::from_utf8(&bytes).unwrap();

        assert!(
            wire.contains("filename*=UTF-8''r%C3%A9sum%C3%A9.txt"),
            "got: {wire}"
        );
        assert!(wire.contains("content-range: bytes 0-4/10"), "got: {wire}");
        assert!(wire.contains("x-custom-trace: trace-abc"), "got: {wire}");
    }

    #[tokio::test]
    async fn into_outgoing_propagates_parse_error() {
        use crate::http::IntoResponse;
        use http_body_util::BodyExt;

        // Truncated payload: opening boundary + headers but no closing boundary.
        // multer should surface this as a parse error mid-stream.
        let truncated =
            "--X-BOUNDARY\r\nContent-Disposition: form-data; name=\"f\"\r\n\r\npartial-data";
        let req = Request::get("/")
            .header(CONTENT_TYPE, "multipart/form-data; boundary=X-BOUNDARY")
            .body(HttpBody::full(truncated))
            .unwrap();
        let (parts, body) = req.into_parts();
        let mp = Multipart::from_payload(Payload::Full(&parts, body))
            .await
            .unwrap();

        let resp = mp.into_outgoing().unwrap().into_response().unwrap();
        let result = resp.into_inner().into_body().collect().await;
        assert!(
            result.is_err(),
            "expected body stream to error on truncated multipart"
        );
    }
}
