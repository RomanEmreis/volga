//! Percent-decoding of a request path
//!
//! A path is decoded one segment at a time, after it has been split on the separators it
//! was written with - so an encoded separator, `%2F`, never starts a segment of its own,
//! and a segment is read as the text the client meant. Decoding is strict: a `%` has to be
//! followed by two hex digits, and what the escapes decode to has to be UTF-8.
//!
//! Unlike form decoding, `+` is left as it is: in a path it is a literal plus sign rather
//! than a space (RFC 3986 Section 3.3).

use crate::error::{Error, IntoError};
use memchr::{memchr, memchr_iter};
use std::borrow::Cow;

/// The byte that starts a percent-escape
const ESCAPE: u8 = b'%';

/// A request path that is not a valid one: a `%` not followed by two hex digits, or escapes
/// that do not decode to UTF-8
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MalformedPath;

/// Answers `400`: a path that does not decode is not a valid request target
impl IntoError for MalformedPath {
    #[inline]
    fn into_error(self) -> Error {
        Error::client_error("Path parsing error: malformed percent-encoding in the request path")
    }
}

/// Returns `true` when `value` carries a `%`, so it may need decoding.
#[inline(always)]
pub(crate) fn is_encoded(value: &str) -> bool {
    memchr(ESCAPE, value.as_bytes()).is_some()
}

/// Returns `true` when `value` carries a well-formed percent-escape.
#[inline]
pub(crate) fn has_escape(value: &str) -> bool {
    let bytes = value.as_bytes();
    memchr_iter(ESCAPE, bytes).any(|at| {
        bytes
            .get(at + 1..at + 3)
            .is_some_and(|hex| hex.iter().all(u8::is_ascii_hexdigit))
    })
}

/// Decodes the percent-escapes of `value`, borrowing it when it carries none.
///
/// # Errors
/// A `%` not followed by two hex digits, or escapes that do not decode to UTF-8.
#[inline]
pub(crate) fn percent_decode(value: &str) -> Result<Cow<'_, str>, MalformedPath> {
    let bytes = value.as_bytes();
    let Some(mut at) = memchr(ESCAPE, bytes) else {
        return Ok(Cow::Borrowed(value));
    };

    // Each escape is three bytes decoding to one, so this is the length of what comes out
    // of a well-formed value, and the string it is collected into is never grown or shrunk
    let escapes = memchr_iter(ESCAPE, bytes).count();
    let mut decoded = Vec::with_capacity(bytes.len().saturating_sub(escapes * 2));

    let mut rest = bytes;
    loop {
        decoded.extend_from_slice(&rest[..at]);

        let [hi, lo] = rest.get(at + 1..at + 3).ok_or(MalformedPath)? else {
            return Err(MalformedPath);
        };
        decoded.push(hex_digit(*hi)? << 4 | hex_digit(*lo)?);

        rest = &rest[at + 3..];
        match memchr(ESCAPE, rest) {
            Some(next) => at = next,
            None => break,
        }
    }
    decoded.extend_from_slice(rest);

    String::from_utf8(decoded)
        .map(Cow::Owned)
        .map_err(|_| MalformedPath)
}

/// The value of one hex digit of an escape
#[inline(always)]
fn hex_digit(byte: u8) -> Result<u8, MalformedPath> {
    match byte {
        b'0'..=b'9' => Ok(byte - b'0'),
        b'a'..=b'f' => Ok(byte - b'a' + 10),
        b'A'..=b'F' => Ok(byte - b'A' + 10),
        _ => Err(MalformedPath),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_borrows_a_value_that_needs_no_decoding() {
        assert!(matches!(
            percent_decode("app.css"),
            Ok(Cow::Borrowed("app.css"))
        ));
        assert!(matches!(percent_decode(""), Ok(Cow::Borrowed(""))));
    }

    #[test]
    fn it_decodes_escapes() {
        for (value, expected) in [
            ("John%20Doe", "John Doe"),
            ("100%25", "100%"),
            ("caf%C3%A9", "caf\u{e9}"),
            ("caf%c3%a9", "caf\u{e9}"),
            ("a%2Fb", "a/b"),
            ("%31", "1"),
            ("%2e%2e", ".."),
            ("%25%32%30", "%20"),
            (
                "%D1%84%D0%B0%D0%B9%D0%BB.txt",
                "\u{0444}\u{0430}\u{0439}\u{043b}.txt",
            ),
        ] {
            assert_eq!(percent_decode(value).unwrap(), expected, "{value}");
        }
    }

    #[test]
    fn it_sizes_the_decoded_value_exactly() {
        let Cow::Owned(decoded) = percent_decode("a%20b%2Fc").unwrap() else {
            panic!("expected an owned value");
        };
        assert_eq!(decoded, "a b/c");
        assert_eq!(decoded.capacity(), decoded.len());
    }

    #[test]
    fn it_leaves_a_plus_alone() {
        assert_eq!(percent_decode("C++%20").unwrap(), "C++ ");
    }

    #[test]
    fn it_rejects_a_malformed_escape() {
        for value in [
            "%", "%2", "%zz", "%2z", "%z2", "app%.css", "a%20%", "bad%zz",
        ] {
            assert_eq!(percent_decode(value), Err(MalformedPath), "{value}");
        }
    }

    #[test]
    fn it_rejects_escapes_that_are_not_utf8() {
        for value in ["%FF", "%FF%FE", "caf%C3", "%C3%28"] {
            assert_eq!(percent_decode(value), Err(MalformedPath), "{value}");
        }
    }

    #[test]
    fn it_tells_a_value_that_may_need_decoding() {
        assert!(is_encoded("a%20b"));
        assert!(is_encoded("100%"));
        assert!(!is_encoded("a b"));
    }

    #[test]
    fn it_finds_a_well_formed_escape() {
        for value in ["a%20b", "%2F", "100%25", "x%zz%41"] {
            assert!(has_escape(value), "{value}");
        }
        for value in ["a b", "100%", "%zz", "%2", "caf\u{e9}"] {
            assert!(!has_escape(value), "{value}");
        }
    }

    #[test]
    fn it_answers_a_malformed_path_with_400() {
        let error = Error::from(MalformedPath);
        assert_eq!(error.status, 400);
    }
}
