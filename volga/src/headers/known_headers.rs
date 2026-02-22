//! Common presets for the most used HTTP headers

use super::{
    Header, 
    ContentType, 
    CacheControl, 
    cache_control::{NO_CACHE, NO_STORE, PUBLIC, PRIVATE}
};

use mime::{
    TEXT_PLAIN, TEXT_PLAIN_UTF_8, 
    TEXT_HTML, TEXT_HTML_UTF_8,
    TEXT_EVENT_STREAM,
    APPLICATION_JSON, APPLICATION_WWW_FORM_URLENCODED, 
    APPLICATION_OCTET_STREAM
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

#[cfg(test)]
mod tests {
    use crate::headers::{CacheControl, ContentType, Header, FromHeaders};

    fn assert_header_value<T>(h: Header<T>, expected: &str)
    where
        T: FromHeaders
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
}