//! Common presets for the most used HTTP headers

use super::{
    CacheControl, ContentType, Header,
    cache_control::{NO_CACHE, NO_STORE, PRIVATE, PUBLIC},
};

#[cfg(feature = "static-files")]
use {super::HeaderValue, std::sync::LazyLock};

use mime::{
    APPLICATION_JSON, APPLICATION_OCTET_STREAM, APPLICATION_WWW_FORM_URLENCODED, TEXT_EVENT_STREAM,
    TEXT_HTML, TEXT_HTML_UTF_8, TEXT_PLAIN, TEXT_PLAIN_UTF_8,
};

impl ContentType {
    /// Creates a `text/plain` [`Header<ContentType>`]
    #[inline]
    pub fn text() -> Header<Self> {
        Self::from_static(TEXT_PLAIN.as_ref())
    }

    /// Creates a `text/plain; charset=utf-8` [`Header<ContentType>`]
    #[inline]
    pub fn text_utf_8() -> Header<Self> {
        Self::from_static(TEXT_PLAIN_UTF_8.as_ref())
    }

    /// Creates a `text/html` [`Header<ContentType>`]
    #[inline]
    pub fn html() -> Header<Self> {
        Self::from_static(TEXT_HTML.as_ref())
    }

    /// Creates a `text/html; charset=utf-8` [`Header<ContentType>`]
    #[inline]
    pub fn html_utf_8() -> Header<Self> {
        Self::from_static(TEXT_HTML_UTF_8.as_ref())
    }

    /// Creates a `application/json` [`Header<ContentType>`]
    #[inline]
    pub fn json() -> Header<Self> {
        Self::from_static(APPLICATION_JSON.as_ref())
    }

    /// Creates a `application/x-www-form-urlencoded` [`Header<ContentType>`]
    #[inline]
    pub fn form() -> Header<Self> {
        Self::from_static(APPLICATION_WWW_FORM_URLENCODED.as_ref())
    }

    /// Creates a `event-stream` [`Header<ContentType>`]
    #[inline]
    pub fn events() -> Header<Self> {
        Self::from_static(TEXT_EVENT_STREAM.as_ref())
    }

    /// Creates a `text/plain` [`Header<ContentType>`]
    #[inline]
    pub fn stream() -> Header<Self> {
        Self::from_static(APPLICATION_OCTET_STREAM.as_ref())
    }

    /// Creates a `multipart/form-data; boundary=...` [`Header<ContentType>`].
    #[inline]
    pub fn multipart_form_data(boundary: &str) -> Header<Self> {
        Self::multipart_custom("form-data", boundary)
            .expect("`form-data` is a static, valid subtype")
    }

    /// Creates a `multipart/mixed; boundary=...` [`Header<ContentType>`].
    #[inline]
    pub fn multipart_mixed(boundary: &str) -> Header<Self> {
        Self::multipart_custom("mixed", boundary).expect("`mixed` is a static, valid subtype")
    }

    /// Creates a `multipart/byteranges; boundary=...` [`Header<ContentType>`].
    #[inline]
    pub fn multipart_byte_ranges(boundary: &str) -> Header<Self> {
        Self::multipart_custom("byteranges", boundary)
            .expect("`byteranges` is a static, valid subtype")
    }

    /// Creates a `multipart/<subtype>; boundary=...` [`Header<ContentType>`].
    /// Boundary must already be RFC 2046 Section 5.1.1 compliant; use `Multipart::with_boundary`
    /// for validation. RFC 2045 tspecials in the boundary (e.g. `:` or space) trigger
    /// quoting of the parameter value so downstream parsers can extract it.
    /// Returns `Err` if `subtype` contains bytes that are invalid in an HTTP header
    /// value (e.g. CR/LF) - the `subtype` is generally caller-controlled runtime input.
    pub fn multipart_custom(
        subtype: &str,
        boundary: &str,
    ) -> Result<Header<Self>, crate::error::Error> {
        let value = if boundary_needs_quoting(boundary) {
            format!("multipart/{subtype}; boundary=\"{boundary}\"")
        } else {
            format!("multipart/{subtype}; boundary={boundary}")
        };
        Self::from_bytes(value.as_bytes())
    }
}

/// Returns `true` if `value` contains any RFC 2045 tspecial or whitespace and must
/// therefore be wrapped in `"..."` when used as a Content-Type parameter value.
/// The validated boundary alphabet excludes `"` and `\`, so simple wrapping is safe.
#[inline]
fn boundary_needs_quoting(value: &str) -> bool {
    value.bytes().any(|b| {
        matches!(
            b,
            b' ' | b'\t'
                | b'('
                | b')'
                | b'<'
                | b'>'
                | b'@'
                | b','
                | b';'
                | b':'
                | b'\\'
                | b'"'
                | b'/'
                | b'['
                | b']'
                | b'?'
                | b'='
        )
    })
}

impl CacheControl {
    /// `Cache-Control: no-cache`
    ///
    /// Forces caches to revalidate before using a stored response.
    #[inline]
    pub fn no_cache() -> Header<Self> {
        Self::from_static(NO_CACHE)
    }

    /// `Cache-Control: no-store`
    ///
    /// Prevents any caching (disk or memory).
    #[inline]
    pub fn no_store() -> Header<Self> {
        Self::from_static(NO_STORE)
    }

    /// `Cache-Control: max-age=0`
    ///
    /// Response is immediately stale.
    #[inline]
    pub fn max_age_0() -> Header<Self> {
        Self::from_static("max-age=0")
    }

    /// `Cache-Control: public`
    #[inline]
    pub fn public() -> Header<Self> {
        Self::from_static(PUBLIC)
    }

    /// `Cache-Control: private`
    #[inline]
    pub fn private() -> Header<Self> {
        Self::from_static(PRIVATE)
    }
}

/// The rendered form of [`CacheControl::ASSET`].
///
/// The policy is the single source of truth, so this is built from it rather than spelled
/// out. Rendering allocates, which the `from_static` presets around here do not, so it is
/// done once on first use and handed out as a [`HeaderValue`] clone - a refcount bump.
#[cfg(feature = "static-files")]
static ASSET: LazyLock<HeaderValue> = LazyLock::new(|| render(CacheControl::ASSET));

/// The rendered form of [`CacheControl::SHELL`], see [`ASSET`].
#[cfg(feature = "static-files")]
static SHELL: LazyLock<HeaderValue> = LazyLock::new(|| render(CacheControl::SHELL));

/// Renders a [`CacheControl`] into the header value it describes.
///
/// [`CacheControl`] writes out nothing but directive names, `=` and decimal digits, so the
/// result is always a valid header value and the failure branch is unreachable.
#[cfg(feature = "static-files")]
fn render(cache_control: CacheControl) -> HeaderValue {
    cache_control
        .try_into()
        .expect("`CacheControl` renders only visible ASCII")
}

#[cfg(feature = "static-files")]
impl CacheControl {
    /// `Cache-Control: max-age=86400, public, immutable`
    ///
    /// The ready header form of [`CacheControl::ASSET`] - what the static file server applies
    /// to a file addressed by a content-hashed name. Reach for the constant instead when the
    /// policy is to be narrowed before it is sent.
    #[inline]
    pub fn asset() -> Header<Self> {
        Header::from_ref(&ASSET)
    }

    /// `Cache-Control: no-cache`
    ///
    /// The ready header form of [`CacheControl::SHELL`] - what the static file server applies
    /// to a file addressed by a stable name, the index and the fallback one. Reach for the
    /// constant instead when the policy is to be narrowed before it is sent.
    ///
    /// This renders the same as [`CacheControl::no_cache`] today, and is not an alias for it:
    /// that one names a directive, this one names the role the directive was chosen for.
    #[inline]
    pub fn shell() -> Header<Self> {
        Header::from_ref(&SHELL)
    }
}

#[cfg(test)]
mod tests {
    use crate::headers::{CacheControl, ContentType, FromHeaders, Header};

    fn assert_header_value<T>(h: Header<T>, expected: &str)
    where
        T: FromHeaders,
    {
        // HeaderValue should always be valid ASCII for these static constants
        let v = h.as_str().expect("header value must be valid ASCII");
        assert_eq!(v, expected);
    }

    #[test]
    fn it_creates_content_type_text() {
        assert_header_value(ContentType::text(), "text/plain");
    }

    #[test]
    fn it_creates_content_type_text_utf_8() {
        assert_header_value(ContentType::text_utf_8(), "text/plain; charset=utf-8");
    }

    #[test]
    fn it_creates_content_type_html() {
        assert_header_value(ContentType::html(), "text/html");
    }

    #[test]
    fn it_creates_content_type_html_utf_8() {
        assert_header_value(ContentType::html_utf_8(), "text/html; charset=utf-8");
    }

    #[test]
    fn it_creates_content_type_json() {
        assert_header_value(ContentType::json(), "application/json");
    }

    #[test]
    fn it_creates_content_type_form() {
        assert_header_value(ContentType::form(), "application/x-www-form-urlencoded");
    }

    #[test]
    fn it_creates_content_type_events() {
        // SSE: no charset; UTF-8 is implied by spec
        assert_header_value(ContentType::events(), "text/event-stream");
    }

    #[test]
    fn it_creates_content_type_stream() {
        assert_header_value(ContentType::stream(), "application/octet-stream");
    }

    #[test]
    fn it_creates_cache_control_no_cache() {
        assert_header_value(CacheControl::no_cache(), "no-cache");
    }

    #[test]
    fn it_creates_cache_control_no_store() {
        assert_header_value(CacheControl::no_store(), "no-store");
    }

    #[test]
    fn it_creates_cache_control_max_age_0() {
        assert_header_value(CacheControl::max_age_0(), "max-age=0");
    }

    #[test]
    fn it_creates_cache_control_public() {
        assert_header_value(CacheControl::public(), "public");
    }

    #[test]
    fn it_creates_cache_control_private() {
        assert_header_value(CacheControl::private(), "private");
    }

    #[cfg(feature = "static-files")]
    #[test]
    fn it_creates_cache_control_asset() {
        assert_header_value(CacheControl::asset(), "max-age=86400, public, immutable");
    }

    #[cfg(feature = "static-files")]
    #[test]
    fn it_creates_cache_control_shell() {
        assert_header_value(CacheControl::shell(), "no-cache");
    }
}

#[cfg(test)]
mod multipart_content_type_tests {
    use super::ContentType;

    #[test]
    fn form_data_with_boundary() {
        let h = ContentType::multipart_form_data("X-BOUNDARY");
        assert_eq!(h.as_ref(), "multipart/form-data; boundary=X-BOUNDARY");
    }

    #[test]
    fn mixed_with_boundary() {
        let h = ContentType::multipart_mixed("XYZ");
        assert_eq!(h.as_ref(), "multipart/mixed; boundary=XYZ");
    }

    #[test]
    fn byteranges_with_boundary() {
        let h = ContentType::multipart_byte_ranges("abc");
        assert_eq!(h.as_ref(), "multipart/byteranges; boundary=abc");
    }

    #[test]
    fn custom_subtype() {
        let h = ContentType::multipart_custom("alternative", "abc").unwrap();
        assert_eq!(h.as_ref(), "multipart/alternative; boundary=abc");
    }

    #[test]
    fn custom_subtype_rejects_invalid_header_bytes() {
        // CR/LF in the subtype must surface as an error, not panic.
        let err = ContentType::multipart_custom("evil\r\ninjected", "abc").unwrap_err();
        assert!(!format!("{err}").is_empty());
    }

    #[test]
    fn boundary_with_tspecials_is_quoted() {
        // ':' is a tspecial - must be wrapped in quotes per RFC 2045.
        let h = ContentType::multipart_form_data("a:b");
        assert_eq!(h.as_ref(), "multipart/form-data; boundary=\"a:b\"");
    }

    #[test]
    fn boundary_with_internal_space_is_quoted() {
        let h = ContentType::multipart_form_data("with space");
        assert_eq!(h.as_ref(), "multipart/form-data; boundary=\"with space\"");
    }

    #[test]
    fn plain_token_boundary_remains_unquoted() {
        let h = ContentType::multipart_form_data("plain-token_42");
        assert_eq!(h.as_ref(), "multipart/form-data; boundary=plain-token_42");
    }
}
