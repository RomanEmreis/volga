//! Outgoing multipart parts.

use bytes::Bytes;
use futures_util::stream::BoxStream;

use crate::error::Error;

/// Body of an outgoing multipart [`Part`]. Either eager bytes or a streaming source.
pub enum PartBody {
    /// Materialized bytes ready to be written.
    Bytes(Bytes),
    /// Streaming body, drained chunk-by-chunk during encoding.
    Stream(BoxStream<'static, Result<Bytes, Error>>),
}

impl std::fmt::Debug for PartBody {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bytes(b) => f.debug_tuple("Bytes").field(b).finish(),
            Self::Stream(_) => f.debug_struct("Stream").finish_non_exhaustive(),
        }
    }
}

impl From<Bytes> for PartBody {
    #[inline]
    fn from(b: Bytes) -> Self {
        Self::Bytes(b)
    }
}

impl From<&'static [u8]> for PartBody {
    #[inline]
    fn from(b: &'static [u8]) -> Self {
        Self::Bytes(Bytes::from_static(b))
    }
}

impl From<Vec<u8>> for PartBody {
    #[inline]
    fn from(v: Vec<u8>) -> Self {
        Self::Bytes(Bytes::from(v))
    }
}

impl From<&'static str> for PartBody {
    #[inline]
    fn from(s: &'static str) -> Self {
        Self::Bytes(Bytes::from_static(s.as_bytes()))
    }
}

impl From<String> for PartBody {
    #[inline]
    fn from(s: String) -> Self {
        Self::Bytes(Bytes::from(s))
    }
}

use crate::headers::{ContentDisposition, ContentType, FromHeaders, Header, HeaderMap};

const INVALID_HEADER_BYTES_MSG: &str = "invalid bytes in part name or filename (e.g. CR/LF); use the corresponding `try_*` constructor for untrusted input";

/// An outgoing multipart part — either form-data, byteranges, or any RFC 2046 subtype.
///
/// Construct with [`Part::new`] (bare) or one of the form-data convenience constructors:
/// [`Part::text`], [`Part::bytes`], [`Part::file`], [`Part::stream`]. Use the `with_*`
/// builder methods to attach headers.
#[derive(Debug)]
pub struct Part {
    pub(super) content_type: Option<Header<ContentType>>,
    pub(super) content_disposition: Option<Header<ContentDisposition>>,
    pub(super) extra: Option<HeaderMap>,
    pub(super) body: PartBody,
}

impl Part {
    /// Constructs a bare part with no headers — for `multipart/byteranges`,
    /// `multipart/mixed`, or any subtype where Content-Disposition is not appropriate.
    pub fn new(body: impl Into<PartBody>) -> Self {
        Self {
            content_type: None,
            content_disposition: None,
            extra: None,
            body: body.into(),
        }
    }

    /// Constructs a form-data text field.
    /// Sets `Content-Type: text/plain; charset=utf-8` and `Content-Disposition: form-data; name="<name>"`.
    ///
    /// # Panics
    /// if `name` contains bytes that are invalid in an HTTP header value (e.g. CR/LF).
    /// For untrusted input, use [`Part::try_text`].
    pub fn text(name: impl AsRef<str>, value: impl Into<String>) -> Self {
        Self::try_text(name, value).expect(INVALID_HEADER_BYTES_MSG)
    }

    /// Fallible counterpart of [`Part::text`]. Returns `Err` if `name` contains bytes
    /// that are invalid in an HTTP header value.
    pub fn try_text(name: impl AsRef<str>, value: impl Into<String>) -> Result<Self, Error> {
        let body: PartBody = value.into().into();
        Ok(Self {
            content_type: Some(ContentType::text_utf_8()),
            content_disposition: Some(make_form_disposition(name.as_ref(), None)?),
            extra: None,
            body,
        })
    }

    /// Constructs a form-data binary field (no filename).
    /// Sets `Content-Type: application/octet-stream` and `Content-Disposition: form-data; name="<name>"`.
    ///
    /// # Panics
    /// if `name` contains bytes that are invalid in an HTTP header value (e.g. CR/LF).
    /// For untrusted input, use [`Part::try_bytes`].
    pub fn bytes(name: impl AsRef<str>, bytes: impl Into<Bytes>) -> Self {
        Self::try_bytes(name, bytes).expect(INVALID_HEADER_BYTES_MSG)
    }

    /// Fallible counterpart of [`Part::bytes`]. Returns `Err` if `name` contains bytes
    /// that are invalid in an HTTP header value.
    pub fn try_bytes(name: impl AsRef<str>, bytes: impl Into<Bytes>) -> Result<Self, Error> {
        Ok(Self {
            content_type: Some(ContentType::stream()),
            content_disposition: Some(make_form_disposition(name.as_ref(), None)?),
            extra: None,
            body: PartBody::Bytes(bytes.into()),
        })
    }

    /// Constructs a form-data file part. Content-Type is guessed from the filename
    /// extension via [`mime_guess`], falling back to `application/octet-stream`.
    ///
    /// # Panics
    /// if `name` or `filename` contains bytes that are invalid in an HTTP header value.
    /// For untrusted input, use [`Part::try_file`].
    pub fn file(name: impl AsRef<str>, filename: impl AsRef<str>, bytes: impl Into<Bytes>) -> Self {
        Self::try_file(name, filename, bytes).expect(INVALID_HEADER_BYTES_MSG)
    }

    /// Fallible counterpart of [`Part::file`]. Returns `Err` if `name` or `filename`
    /// contains bytes that are invalid in an HTTP header value.
    pub fn try_file(
        name: impl AsRef<str>,
        filename: impl AsRef<str>,
        bytes: impl Into<Bytes>,
    ) -> Result<Self, Error> {
        let filename_str = filename.as_ref();
        let mime = mime_guess::from_path(filename_str).first_or_octet_stream();
        let ct = Header::<ContentType>::from_bytes(mime.essence_str().as_bytes())
            .unwrap_or_else(|_| ContentType::stream());
        Ok(Self {
            content_type: Some(ct),
            content_disposition: Some(make_form_disposition(name.as_ref(), Some(filename_str))?),
            extra: None,
            body: PartBody::Bytes(bytes.into()),
        })
    }

    /// Constructs a streaming form-data file part. The caller supplies the Content-Type
    /// since filename-based guessing is not always meaningful for streams.
    ///
    /// # Panics
    /// if `name` or `filename` contains bytes that are invalid in an HTTP header value.
    /// For untrusted input, use [`Part::try_stream`].
    pub fn stream<S>(
        name: impl AsRef<str>,
        filename: impl AsRef<str>,
        content_type: Header<ContentType>,
        stream: S,
    ) -> Self
    where
        S: futures_util::Stream<Item = Result<Bytes, Error>> + Send + 'static,
    {
        Self::try_stream(name, filename, content_type, stream).expect(INVALID_HEADER_BYTES_MSG)
    }

    /// Fallible counterpart of [`Part::stream`]. Returns `Err` if `name` or `filename`
    /// contains bytes that are invalid in an HTTP header value.
    pub fn try_stream<S>(
        name: impl AsRef<str>,
        filename: impl AsRef<str>,
        content_type: Header<ContentType>,
        stream: S,
    ) -> Result<Self, Error>
    where
        S: futures_util::Stream<Item = Result<Bytes, Error>> + Send + 'static,
    {
        Ok(Self {
            content_type: Some(content_type),
            content_disposition: Some(make_form_disposition(
                name.as_ref(),
                Some(filename.as_ref()),
            )?),
            extra: None,
            body: PartBody::Stream(Box::pin(stream)),
        })
    }

    /// Overrides the Content-Type header.
    pub fn with_content_type(mut self, ct: Header<ContentType>) -> Self {
        self.content_type = Some(ct);
        self
    }

    /// Sets the Content-Disposition header to `form-data; name="<name>"[; filename="<filename>"]`.
    ///
    /// # Panics
    /// if `name` or `filename` contains bytes that are invalid in an HTTP header value.
    /// For untrusted input, use [`Part::try_with_disposition`].
    pub fn with_disposition(self, name: &str, filename: Option<&str>) -> Self {
        self.try_with_disposition(name, filename)
            .expect(INVALID_HEADER_BYTES_MSG)
    }

    /// Fallible counterpart of [`Part::with_disposition`]. Returns `Err` if `name` or
    /// `filename` contains bytes that are invalid in an HTTP header value.
    pub fn try_with_disposition(self, name: &str, filename: Option<&str>) -> Result<Self, Error> {
        Ok(self.with_disposition_raw(make_form_disposition(name, filename)?))
    }

    /// Sets the Content-Disposition header verbatim — for cases not covered by the
    /// form-data builder (RFC 5987 encoding, alternative dispositions, etc).
    pub fn with_disposition_raw(mut self, cd: Header<ContentDisposition>) -> Self {
        self.content_disposition = Some(cd);
        self
    }

    /// Appends a typed header. The `extra` map is allocated lazily on first call.
    pub fn with_header<T: FromHeaders>(mut self, header: Header<T>) -> Self {
        let map = self.extra.get_or_insert_with(HeaderMap::new);
        let name = header.name();
        map.insert(name, header.into_inner());
        self
    }

    /// Appends a raw header — escape hatch for one-off headers that don't have
    /// a dedicated marker type.
    pub fn with_header_raw(
        mut self,
        name: crate::headers::HeaderName,
        value: crate::headers::HeaderValue,
    ) -> Self {
        let map = self.extra.get_or_insert_with(HeaderMap::new);
        map.insert(name, value);
        self
    }

    /// `pub(super)` accessor for the encoder.
    #[inline]
    pub(super) fn part_content_type(&self) -> Option<&Header<ContentType>> {
        self.content_type.as_ref()
    }

    /// `pub(super)` accessor for the encoder.
    #[inline]
    pub(super) fn part_content_disposition(&self) -> Option<&Header<ContentDisposition>> {
        self.content_disposition.as_ref()
    }

    /// `pub(super)` accessor for the encoder.
    #[inline]
    pub(super) fn part_extras(&self) -> Option<&HeaderMap> {
        self.extra.as_ref()
    }

    /// `pub(super)` consumer for the encoder.
    #[inline]
    pub(super) fn into_body(self) -> PartBody {
        self.body
    }
}

/// Builds a `Content-Disposition` header value of the form
/// `form-data; name="<name>"[; filename="<filename>"]` with backslash-escaping
/// of `"` and `\` in the parameter values per RFC 7578 / RFC 2183.
/// Returns `Err` if `name` or `filename` contains bytes that aren't valid in an HTTP
/// header value (e.g. CR / LF / NUL).
pub(super) fn make_form_disposition(
    name: &str,
    filename: Option<&str>,
) -> Result<Header<ContentDisposition>, Error> {
    let mut s = String::with_capacity(32);
    s.push_str("form-data; name=\"");
    escape_disposition_param(&mut s, name);
    s.push('"');
    if let Some(fname) = filename {
        s.push_str("; filename=\"");
        escape_disposition_param(&mut s, fname);
        s.push('"');
    }
    Header::<ContentDisposition>::from_bytes(s.as_bytes())
}

fn escape_disposition_param(out: &mut String, value: &str) {
    for ch in value.chars() {
        if ch == '"' || ch == '\\' {
            out.push('\\');
        }
        out.push(ch);
    }
}

#[cfg(test)]
mod tests {
    use super::{Part, PartBody};
    use crate::headers::ContentType;
    use bytes::Bytes;
    use futures_util::stream;

    #[test]
    fn from_bytes() {
        let pb: PartBody = Bytes::from_static(b"hello").into();
        assert!(matches!(pb, PartBody::Bytes(b) if b == "hello"));
    }

    #[test]
    fn from_static_slice() {
        let pb: PartBody = (b"hello" as &'static [u8]).into();
        assert!(matches!(pb, PartBody::Bytes(b) if b == "hello"));
    }

    #[test]
    fn from_vec() {
        let pb: PartBody = vec![1u8, 2, 3].into();
        assert!(matches!(pb, PartBody::Bytes(b) if b.as_ref() == [1, 2, 3]));
    }

    #[test]
    fn from_static_str() {
        let pb: PartBody = "hello".into();
        assert!(matches!(pb, PartBody::Bytes(b) if b == "hello"));
    }

    #[test]
    fn from_string() {
        let pb: PartBody = String::from("hello").into();
        assert!(matches!(pb, PartBody::Bytes(b) if b == "hello"));
    }

    #[test]
    fn new_creates_bare_part_no_headers() {
        let p = Part::new("hello");
        assert!(p.content_type.is_none());
        assert!(p.content_disposition.is_none());
        assert!(p.extra.is_none());
    }

    #[test]
    fn text_sets_text_plain_utf8_and_disposition() {
        let p = Part::text("name", "value");
        let ct = p.content_type.expect("ct set");
        assert_eq!(ct.as_ref(), "text/plain; charset=utf-8");
        let cd = p.content_disposition.expect("cd set");
        assert_eq!(cd.as_ref(), "form-data; name=\"name\"");
    }

    #[test]
    fn bytes_sets_octet_stream_and_disposition() {
        let p = Part::bytes("blob", Bytes::from_static(b"\x01\x02\x03"));
        let ct = p.content_type.expect("ct set");
        assert_eq!(ct.as_ref(), "application/octet-stream");
        let cd = p.content_disposition.expect("cd set");
        assert_eq!(cd.as_ref(), "form-data; name=\"blob\"");
    }

    #[test]
    fn disposition_escapes_quote_and_backslash() {
        let p = Part::text(r#"weird"name\with\\backslashes"#, "x");
        let cd = p.content_disposition.expect("cd set");
        assert_eq!(
            cd.as_ref(),
            r#"form-data; name="weird\"name\\with\\\\backslashes""#
        );
    }

    #[test]
    fn try_file_rejects_invalid_header_bytes() {
        let err =
            Part::try_file("name\r\nfield", "ev\nil.txt", Bytes::from_static(b"x")).unwrap_err();
        assert!(
            !format!("{err}").is_empty(),
            "expected an error describing the invalid header value"
        );
    }

    #[test]
    #[should_panic(expected = "invalid bytes in part name")]
    fn text_panics_on_invalid_header_bytes() {
        let _ = Part::text("name\r\nfield", "x");
    }

    #[test]
    fn file_guesses_pdf_mime() {
        let p = Part::file("doc", "report.pdf", Bytes::from_static(b"%PDF-1.4\n"));
        let ct = p.content_type.expect("ct set");
        assert_eq!(ct.as_ref(), "application/pdf");
        let cd = p.content_disposition.expect("cd set");
        assert_eq!(
            cd.as_ref(),
            "form-data; name=\"doc\"; filename=\"report.pdf\""
        );
    }

    #[test]
    fn file_falls_back_to_octet_stream_for_unknown_extension() {
        let p = Part::file("doc", "weird.zzz", Bytes::from_static(b"x"));
        let ct = p.content_type.expect("ct set");
        assert_eq!(ct.as_ref(), "application/octet-stream");
    }

    #[tokio::test]
    async fn stream_keeps_caller_supplied_content_type() {
        let body = stream::iter(vec![Ok::<_, crate::error::Error>(Bytes::from_static(b"x"))]);
        let p = Part::stream("doc", "log.txt", ContentType::text_utf_8(), body);
        let ct = p.content_type.expect("ct set");
        assert_eq!(ct.as_ref(), "text/plain; charset=utf-8");
        let cd = p.content_disposition.expect("cd set");
        assert_eq!(cd.as_ref(), "form-data; name=\"doc\"; filename=\"log.txt\"");
        assert!(matches!(p.body, PartBody::Stream(_)));
    }

    use crate::headers::{ContentDisposition, ContentEncoding, HeaderName, HeaderValue};

    #[test]
    fn with_content_type_overrides() {
        let p = Part::text("n", "v").with_content_type(ContentType::html_utf_8());
        assert_eq!(p.content_type.unwrap().as_ref(), "text/html; charset=utf-8");
    }

    #[test]
    fn with_disposition_replaces() {
        let p = Part::text("old", "v").with_disposition("new", Some("file.txt"));
        assert_eq!(
            p.content_disposition.unwrap().as_ref(),
            "form-data; name=\"new\"; filename=\"file.txt\""
        );
    }

    #[test]
    fn try_with_disposition_returns_err_on_invalid_bytes() {
        let err = Part::new("body")
            .try_with_disposition("name\r\nfield", None)
            .unwrap_err();
        assert!(!format!("{err}").is_empty());
    }

    #[test]
    fn with_disposition_raw_passes_through() {
        let cd = crate::headers::Header::<ContentDisposition>::from_static(
            r#"attachment; filename="x""#,
        );
        let p = Part::new("body").with_disposition_raw(cd);
        assert_eq!(
            p.content_disposition.unwrap().as_ref(),
            r#"attachment; filename="x""#
        );
    }

    #[test]
    fn with_header_typed_lazily_allocates_extra() {
        let p = Part::text("n", "v");
        assert!(p.extra.is_none(), "extra is None until first custom header");

        let enc = crate::headers::Header::<ContentEncoding>::from_static("gzip");
        let p = p.with_header(enc);
        assert!(p.extra.is_some(), "extra allocated after first with_header");
        let map = p.extra.unwrap();
        assert_eq!(map.get("content-encoding").unwrap(), "gzip");
    }

    #[test]
    fn with_header_raw_lazily_allocates_extra() {
        let p = Part::text("n", "v");
        assert!(p.extra.is_none());
        let p = p.with_header_raw(
            HeaderName::from_static("x-custom"),
            HeaderValue::from_static("y"),
        );
        assert_eq!(p.extra.unwrap().get("x-custom").unwrap(), "y");
    }
}
