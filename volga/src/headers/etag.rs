//! Utilities for ETAG header

use super::{ETAG, FromHeaders, Header, HeaderMap, HeaderName, HeaderValue};
use crate::error::Error;
use std::{
    borrow::Cow,
    fmt::Display,
    ops::Deref
};

#[cfg(feature = "static-files")]
use sha1::{Sha1, Digest};
#[cfg(feature = "static-files")]
use std::fs::Metadata;
#[cfg(feature = "static-files")]
use std::time::UNIX_EPOCH;

/// Represents Entity Tag (ETag) value
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ETag {
    inner: Cow<'static, str>,
}

/// Represents Entity Tag (ETag) reference
#[derive(Debug, Clone, Copy)]
pub(crate) struct ETagRef<'a> {
    raw: &'a str,
    start: usize,
    end: usize,
    weak: bool,
}

impl FromHeaders for ETag {
    const NAME: HeaderName = ETAG;

    #[inline]
    fn from_headers(headers: &HeaderMap) -> Option<&HeaderValue> {
        headers.get(Self::NAME)
    }
}

#[cfg(feature = "static-files")]
impl TryFrom<&Metadata> for ETag {
    type Error = crate::error::Error;
    
    #[inline]
    fn try_from(metadata: &Metadata) -> Result<Self, Self::Error> {
        let mut hasher = Sha1::new();
        hasher.update(metadata.len().to_string());
        
        let mod_time = metadata.modified()?;
        let duration = mod_time.duration_since(UNIX_EPOCH)
            .map_err(Self::Error::server_error)?;

        hasher.update(duration.as_secs().to_string());
        
        let tag = format!("{:x}", hasher.finalize());
        ETag::try_weak(tag)
    }
}

impl TryFrom<String> for ETag {
    type Error = Error;

    #[inline]
    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_strong(value)
    }
}

impl Deref for ETag {
    type Target = str;

    #[inline]
    fn deref(&self) -> &Self::Target {
        self.inner.as_ref()
    }
}

impl AsRef<str> for ETag {
    #[inline]
    fn as_ref(&self) -> &str {
        self.inner.as_ref()
    }
}

impl Display for ETag {
    #[inline]
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_ref())
    }
}

impl TryFrom<&ETag> for HeaderValue {
    type Error = Error;

    #[inline]
    fn try_from(v: &ETag) -> Result<HeaderValue, Error> {
        HeaderValue::from_str(v.as_ref())
            .map_err(|_| Error::client_error("Invalid ETag"))
    }
}

impl ETag {
    /// Creates a strong [`ETag`]
    /// 
    /// # Panics
    /// 
    /// if a tag contains control chars: " or \
    #[inline]
    pub fn strong(tag: impl AsRef<str>) -> Self {
        Self::try_strong(tag).expect("invalid ETag tag")
    }

    /// Creates a weak [`ETag`]
    /// 
    /// # Panics
    /// 
    /// if a tag contains control chars: " or \
    #[inline]
    pub fn weak(tag: impl AsRef<str>) -> Self {
        Self::try_weak(tag).expect("invalid ETag tag")
    }

    /// Creates a strong [`ETag`]
    /// 
    /// Validation: forbid CTL + CRLF, forbid `"` and `\` in tag.
    #[inline]
    pub fn try_strong(tag: impl AsRef<str>) -> Result<Self, Error> {
        let tag = tag.as_ref();
        validate_tag(tag)?;
        Ok(Self { inner: Cow::Owned(format!("\"{tag}\"")) })
    }

    ///  Creates a weak [`ETag`]
    /// 
    ///  Validation: forbid CTL + CRLF, forbid `"` and `\` in tag.
    #[inline]
    pub fn try_weak(tag: impl AsRef<str>) -> Result<Self, Error> {
        let tag = tag.as_ref();
        validate_tag(tag)?;
        Ok(Self { inner: Cow::Owned(format!("W/\"{tag}\"")) })
    }

    /// Parse raw ETag header value: `"..."` or `W/"..."` only.
    #[inline]
    pub fn parse(raw: impl AsRef<str>) -> Result<Self, Error> {
        let raw = raw.as_ref();
        let r = parse_etag_ref(raw)?;
        Ok(Self { inner: Cow::Owned(r.raw.to_owned()) })
    }

    /// Returns true if this etag is a weak
    #[inline]
    pub fn is_weak(&self) -> bool {
        self.inner.as_ref().starts_with("W/\"")
    }

    /// Returns inner tag without quotes (and without W/ prefix).
    /// Assumes the ETag instance is valid (constructed by strong/weak/parse).
    #[inline]
    pub fn tag(&self) -> &str {
        let s = self.inner.as_ref();
        if s.starts_with("W/\"") {
            &s[3..s.len() - 1] // after W/" ... "
        } else {
            &s[1..s.len() - 1] // after " ... "
        }
    }

    /// Strong comparison: both must be strong AND identical.
    #[inline]
    pub fn strong_eq(&self, other: &ETag) -> bool {
        !self.is_weak() && !other.is_weak() && self.inner.as_ref() == other.inner.as_ref()
    }

    /// Weak comparison: compare tags ignoring weakness.
    #[inline]
    pub fn weak_eq(&self, other: &ETag) -> bool {
        self.tag() == other.tag()
    }

    /// Creates a new instance of [`Header<T>`] from a `static str`
    #[inline(always)]
    pub const fn from_static(value: &'static str) -> Header<Self> {
        Header::<Self>::from_static(value)
    }
                
    /// Construct a typed header from bytes (validated).
    #[inline]
    pub fn from_bytes(bytes: &[u8]) -> Result<Header<Self>, Error> {
        Header::<Self>::from_bytes(bytes)
    }

    /// Wrap an owned raw HeaderValue (validated elsewhere).
    #[inline]
    pub fn new(value: HeaderValue) -> Header<Self> {
        Header::<Self>::new(value)
    }

    /// Wrap a borrowed raw HeaderValue (validated elsewhere).
    #[inline]
    pub fn from_ref(value: &HeaderValue) -> Header<Self> {
        Header::<Self>::from_ref(value)
    }
}

impl<'a> ETagRef<'a> {
    #[inline]
    pub(crate) fn parse(raw: &'a str) -> Result<Self, Error> {
        parse_etag_ref(raw)
    }

    #[inline]
    #[allow(unused)]
    pub(crate) fn is_weak(&self) -> bool { self.weak }

    #[inline]
    pub(crate) fn tag(&self) -> &'a str { &self.raw[self.start..self.end] }

    #[inline]
    pub(crate) fn weak_eq_tag(&self, other_tag: &str) -> bool { self.tag() == other_tag }

    #[inline]
    #[allow(unused)]
    pub(crate) fn weak_eq(&self, other: &ETagRef<'_>) -> bool { self.tag() == other.tag() }

    #[inline]
    #[allow(unused)]
    pub(crate) fn strong_eq(&self, other: &ETagRef<'_>) -> bool {
        !self.weak && !other.weak && self.raw == other.raw
    }
}

#[inline]
pub(crate) fn parse_etag_ref(raw: &str) -> Result<ETagRef<'_>, Error> {
    let raw = raw.trim();
    let bytes = raw.as_bytes();

    // reject empty / too short early
    if bytes.len() < 2 {
        return Err(Error::client_error("Invalid ETag"));
    }

    let (weak, start, end) = if raw.starts_with("W/\"") {
        if !raw.ends_with('"') || bytes.len() < 4 {
            return Err(Error::client_error("Invalid weak ETag"));
        }
        (true, 3, bytes.len() - 1)
    } else if raw.starts_with('"') {
        if !raw.ends_with('"') {
            return Err(Error::client_error("Invalid strong ETag"));
        }
        (false, 1, bytes.len() - 1)
    } else {
        return Err(Error::client_error("Invalid ETag"));
    };

    // body must not be empty
    if end <= start {
        return Err(Error::client_error("Invalid ETag"));
    }

    // pragmatic validation: forbid CTL/DEL and forbid `"` and `\`
    for &b in &bytes[start..end] {
        if b <= 31 || b == 127 || b == b'"' || b == b'\\' {
            return Err(Error::client_error("Invalid ETag"));
        }
    }

    Ok(ETagRef { raw, start, end, weak })
}

/// Disallow CRLF + CTL and also disallow `"` and `\` in tag.
/// This keeps construction simple (no escaping rules).
#[inline]
fn validate_tag(tag: &str) -> Result<(), Error> {
    if tag.is_empty() {
        // pick your error ctor
        return Err(Error::client_error("ETag tag is empty"));
    }

    for &b in tag.as_bytes() {
        // CTL (0..=31) and DEL (127)
        if b <= 31 || b == 127 {
            return Err(Error::client_error("ETag tag contains control characters"));
        }
        // forbid quote and backslash (no escaping support)
        if b == b'"' || b == b'\\' {
            return Err(Error::client_error("ETag tag contains invalid characters"));
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::headers::ETag;
    use super::parse_etag_ref;

    #[test]
    fn it_creates_etag() {
        let etag = ETag::strong("foo");
        
        assert_eq!(etag.as_ref(), "\"foo\"");
    }

    #[test]
    fn it_creates_etag_from_string() {
        let etag = ETag::try_from(String::from("foo")).unwrap();

        assert_eq!(etag.as_ref(), "\"foo\"");
    }

    #[test]
    fn it_creates_string_from_etag() {
        let etag = ETag::strong("foo");

        assert_eq!(etag.to_string(), "\"foo\"");
    }

    #[test]
    fn it_compares_etag() {
        let etag1 = ETag::strong("foo");
        let etag2 = ETag::strong("foo");

        assert_eq!(*etag1, *etag2);
    }

    fn assert_ok<T, E>(r: Result<T, E>) -> T {
        match r {
            Ok(v) => v,
            Err(_) => panic!("expected Ok(..)"),
        }
    }

    fn assert_err<T, E>(r: Result<T, E>) {
        if r.is_ok() {
            panic!("expected Err(..)");
        }
    }

    #[test]
    fn try_strong_creates_quoted_value() {
        let etag = assert_ok(ETag::try_strong("foo"));
        assert_eq!(etag.as_ref(), "\"foo\"");
        assert!(!etag.is_weak());
        assert_eq!(etag.tag(), "foo");
    }

    #[test]
    fn try_weak_creates_weak_quoted_value() {
        let etag = assert_ok(ETag::try_weak("foo"));
        assert_eq!(etag.as_ref(), "W/\"foo\"");
        assert!(etag.is_weak());
        assert_eq!(etag.tag(), "foo");
    }

    #[test]
    fn parse_accepts_strong_and_weak_forms() {
        let s = assert_ok(ETag::parse("\"abc\""));
        assert_eq!(s.as_ref(), "\"abc\"");
        assert!(!s.is_weak());
        assert_eq!(s.tag(), "abc");

        let w = assert_ok(ETag::parse("W/\"abc\""));
        assert_eq!(w.as_ref(), "W/\"abc\"");
        assert!(w.is_weak());
        assert_eq!(w.tag(), "abc");
    }

    #[test]
    fn parse_rejects_missing_quotes_or_wrong_prefix() {
        assert_err(ETag::parse("abc"));
        assert_err(ETag::parse("W/abc"));
        assert_err(ETag::parse("\"abc"));
        assert_err(ETag::parse("abc\""));
        assert_err(ETag::parse("w/\"abc\"")); // case-sensitive
    }

    #[test]
    fn try_strong_rejects_empty_tag() {
        assert_err(ETag::try_strong(""));
    }

    #[test]
    fn try_weak_rejects_empty_tag() {
        assert_err(ETag::try_weak(""));
    }

    #[test]
    fn try_strong_rejects_quote_and_backslash() {
        assert_err(ETag::try_strong("a\"b"));
        assert_err(ETag::try_strong("a\\b"));
    }

    #[test]
    fn try_weak_rejects_quote_and_backslash() {
        assert_err(ETag::try_weak("a\"b"));
        assert_err(ETag::try_weak("a\\b"));
    }

    #[test]
    fn try_strong_rejects_control_chars_and_crlf() {
        assert_err(ETag::try_strong("a\nb"));
        assert_err(ETag::try_strong("a\rb"));
        assert_err(ETag::try_strong("a\tb")); // tab is CTL
        assert_err(ETag::try_strong("a\u{0000}b"));
        assert_err(ETag::try_strong("a\u{007F}b")); // DEL
    }

    #[test]
    fn parse_rejects_control_chars_and_inner_quotes() {
        // inner quote not allowed (we don't support escaping)
        assert_err(ETag::parse("\"a\"b\""));
        assert_err(ETag::parse("W/\"a\"b\""));

        // CTL not allowed inside
        assert_err(ETag::parse("\"a\nb\""));
        assert_err(ETag::parse("W/\"a\rb\""));
        assert_err(ETag::parse("\"a\tb\""));
        assert_err(ETag::parse("\"a\u{0000}b\""));
        assert_err(ETag::parse("\"a\u{007F}b\""));
    }

    #[test]
    fn comparisons_work_as_expected() {
        let s1 = assert_ok(ETag::try_strong("v1"));
        let s2 = assert_ok(ETag::try_strong("v1"));
        let s3 = assert_ok(ETag::try_strong("v2"));

        let w1 = assert_ok(ETag::try_weak("v1"));
        let w2 = assert_ok(ETag::try_weak("v1"));
        let w3 = assert_ok(ETag::try_weak("v2"));

        // strong_eq: only true for identical strong etags
        assert!(s1.strong_eq(&s2));
        assert!(!s1.strong_eq(&s3));
        assert!(!s1.strong_eq(&w1));
        assert!(!w1.strong_eq(&w2)); // weak never strong-eq

        // weak_eq: compares tag ignoring weakness
        assert!(s1.weak_eq(&w1));
        assert!(w1.weak_eq(&s2));
        assert!(w1.weak_eq(&w2));
        assert!(!w1.weak_eq(&w3));
        assert!(!s1.weak_eq(&s3));
    }

    #[test]
    #[should_panic]
    fn strong_panics_on_invalid_tag_if_kept_as_expect() {
        let _ = ETag::strong("a\nb");
    }

    #[test]
    #[should_panic]
    fn weak_panics_on_invalid_tag_if_kept_as_expect() {
        let _ = ETag::weak("a\"b");
    }

    #[test]
    fn etag_ref_parses_strong() {
        let r = assert_ok(parse_etag_ref("\"abc\""));
        assert!(!r.is_weak());
        assert_eq!(r.tag(), "abc");
    }

    #[test]
    fn etag_ref_parses_weak() {
        let r = assert_ok(parse_etag_ref("W/\"abc\""));
        assert!(r.is_weak());
        assert_eq!(r.tag(), "abc");
    }

    #[test]
    fn etag_ref_trims_whitespace() {
        let r = assert_ok(parse_etag_ref("  W/\"abc\"  "));
        assert!(r.is_weak());
        assert_eq!(r.tag(), "abc");
    }

    #[test]
    fn etag_ref_rejects_missing_quotes_or_bad_prefix() {
        assert_err(parse_etag_ref("abc"));
        assert_err(parse_etag_ref("W/abc"));
        assert_err(parse_etag_ref("w/\"abc\"")); // case-sensitive
        assert_err(parse_etag_ref("\"abc"));
        assert_err(parse_etag_ref("abc\""));
        assert_err(parse_etag_ref("W/\"abc")); // missing closing quote
    }

    #[test]
    fn etag_ref_rejects_empty_or_too_short() {
        assert_err(parse_etag_ref(""));
        assert_err(parse_etag_ref("\"\""));
        assert_err(parse_etag_ref("W/\"\""));
        assert_err(parse_etag_ref("\"")); // too short
    }

    #[test]
    fn etag_ref_rejects_control_chars_and_del() {
        assert_err(parse_etag_ref("\"a\nb\""));
        assert_err(parse_etag_ref("W/\"a\rb\""));
        assert_err(parse_etag_ref("\"a\tb\""));
        assert_err(parse_etag_ref("\"a\u{0000}b\""));
        assert_err(parse_etag_ref("\"a\u{007F}b\"")); // DEL
    }

    #[test]
    fn etag_ref_rejects_inner_quote_and_backslash() {
        // we do not support escaping -> forbid these
        assert_err(parse_etag_ref("\"a\"b\""));
        assert_err(parse_etag_ref("W/\"a\"b\""));

        assert_err(parse_etag_ref("\"a\\b\""));
        assert_err(parse_etag_ref("W/\"a\\b\""));
    }

    #[test]
    fn etag_ref_tag_slicing_is_correct() {
        let r1 = assert_ok(parse_etag_ref("\"x\""));
        assert_eq!(r1.tag(), "x");

        let r2 = assert_ok(parse_etag_ref("W/\"x\""));
        assert_eq!(r2.tag(), "x");

        let r3 = assert_ok(parse_etag_ref("\"hello\""));
        assert_eq!(r3.tag(), "hello");
    }

    #[test]
    fn etag_ref_comparisons_work() {
        let s1 = assert_ok(parse_etag_ref("\"v1\""));
        let s2 = assert_ok(parse_etag_ref("\"v1\""));
        let s3 = assert_ok(parse_etag_ref("\"v2\""));
        let w1 = assert_ok(parse_etag_ref("W/\"v1\""));
        let w2 = assert_ok(parse_etag_ref("W/\"v1\""));

        // tag equality ignores weakness
        assert!(s1.weak_eq(&w1));
        assert!(w1.weak_eq(&s2));
        assert!(w1.weak_eq(&w2));
        assert!(!w1.weak_eq(&s3));
    }
}