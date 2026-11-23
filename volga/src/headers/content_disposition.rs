//! Builders for the `Content-Disposition` header (RFC 6266, RFC 8187, RFC 7578)
//!
//! Every value volga builds for this header goes through here, so that a file or a field
//! name, which often comes from user input, cannot end the quoted string it is written in
//! and add parameters of its own.

use super::{ContentDisposition, Header};

#[cfg(feature = "multipart")]
use crate::error::Error;

/// Uppercase hex digits for the percent-encoding of RFC 8187 Section 3.2.1.
const HEX: &[u8; 16] = b"0123456789ABCDEF";

impl ContentDisposition {
    /// Creates an `attachment` [`Header<ContentDisposition>`] that tells the client to save
    /// the response as `filename`.
    ///
    /// Any name is safe here, including one taken from user input:
    /// - `filename="..."` is an ASCII fallback (RFC 6266 Section 4.3): `"` and `\` are
    ///   escaped, and every non-ASCII character is replaced by `_`;
    /// - `filename*=UTF-8''...` carries the exact name, percent-encoded (RFC 8187). It is
    ///   added when the name holds a non-ASCII character, `"` or `\`, and user agents
    ///   prefer it to `filename`;
    /// - control characters, CR and LF among them, are replaced by `_` in both.
    ///
    /// # Example
    /// ```
    /// use volga::headers::ContentDisposition;
    ///
    /// let header = ContentDisposition::attachment("report.pdf");
    /// assert_eq!(header.as_ref(), "attachment; filename=\"report.pdf\"");
    ///
    /// let header = ContentDisposition::attachment("résumé.pdf");
    /// assert_eq!(
    ///     header.as_ref(),
    ///     "attachment; filename=\"r_sum_.pdf\"; filename*=UTF-8''r%C3%A9sum%C3%A9.pdf"
    /// );
    /// ```
    #[must_use]
    pub fn attachment(filename: &str) -> Header<Self> {
        with_filename("attachment", filename)
    }

    /// Creates a `form-data; name="..."[; filename="..."]` [`Header<ContentDisposition>`]
    /// for a part of a `multipart/form-data` body.
    ///
    /// `"` and `\` are escaped. Non-ASCII characters are kept as UTF-8 and no `filename*`
    /// is added, as RFC 7578 Section 4.2 requires. Returns `Err` if `name` or `filename`
    /// holds bytes that are invalid in a header value (CR, LF, NUL).
    #[cfg(feature = "multipart")]
    pub(crate) fn form_data(name: &str, filename: Option<&str>) -> Result<Header<Self>, Error> {
        let mut value = String::with_capacity(
            "form-data; name=\"\"; filename=\"\"".len() + name.len() + filename.map_or(0, str::len),
        );
        value.push_str("form-data; name=");
        push_quoted(&mut value, name.chars());
        if let Some(filename) = filename {
            value.push_str("; filename=");
            push_quoted(&mut value, filename.chars());
        }
        Self::from_bytes(value.as_bytes())
    }
}

/// Builds `<kind>; filename="<ascii fallback>"[; filename*=UTF-8''<percent-encoded>]`.
fn with_filename(kind: &'static str, filename: &str) -> Header<ContentDisposition> {
    let mut value = String::with_capacity(kind.len() + "; filename=\"\"".len() + filename.len());
    value.push_str(kind);
    value.push_str("; filename=");
    push_quoted(&mut value, filename.chars().map(ascii_fallback));

    if filename.chars().any(needs_ext_value) {
        value.push_str("; filename*=UTF-8''");
        push_ext_value(&mut value, filename);
    }

    // The value holds visible ASCII and spaces alone, which a header value always accepts;
    // the bare disposition type is there so that this never panics.
    ContentDisposition::from_bytes(value.as_bytes())
        .unwrap_or_else(|_| ContentDisposition::from_static(kind))
}

/// Writes `chars` as a quoted-string (RFC 9110 Section 5.6.4), escaping `"` and `\`.
fn push_quoted(out: &mut String, chars: impl Iterator<Item = char>) {
    out.push('"');
    for ch in chars {
        if matches!(ch, '"' | '\\') {
            out.push('\\');
        }
        out.push(ch);
    }
    out.push('"');
}

/// Maps a character of a file name to the ASCII fallback of `filename`.
#[inline]
fn ascii_fallback(ch: char) -> char {
    if ch.is_ascii() && !ch.is_ascii_control() {
        ch
    } else {
        '_'
    }
}

/// Whether the ASCII fallback loses or escapes this character, so the exact name has to
/// go in `filename*` as well. Control characters are replaced in both, so they do not count.
#[inline]
fn needs_ext_value(ch: char) -> bool {
    matches!(ch, '"' | '\\') || (!ch.is_ascii() && !ch.is_control())
}

/// Writes the value of `filename*` (RFC 8187 Section 3.2.1): the UTF-8 bytes of the name,
/// everything but `attr-char` percent-encoded, control characters replaced by `_`.
fn push_ext_value(out: &mut String, filename: &str) {
    let mut buf = [0u8; 4];
    for ch in filename.chars() {
        let ch = if ch.is_control() { '_' } else { ch };
        for &byte in ch.encode_utf8(&mut buf).as_bytes() {
            if is_attr_char(byte) {
                out.push(char::from(byte));
            } else {
                out.push('%');
                out.push(char::from(HEX[usize::from(byte >> 4)]));
                out.push(char::from(HEX[usize::from(byte & 0x0F)]));
            }
        }
    }
}

/// `attr-char` of RFC 8187 Section 3.2.1.
#[inline]
fn is_attr_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric()
        || matches!(
            byte,
            b'!' | b'#' | b'$' | b'&' | b'+' | b'-' | b'.' | b'^' | b'_' | b'`' | b'|' | b'~'
        )
}

#[cfg(test)]
mod tests {
    use super::ContentDisposition;

    fn attachment(name: &str) -> String {
        ContentDisposition::attachment(name)
            .as_str()
            .expect("an attachment value is visible ASCII")
            .to_owned()
    }

    #[test]
    fn it_writes_a_plain_name_as_is() {
        assert_eq!(
            attachment("report.pdf"),
            "attachment; filename=\"report.pdf\""
        );
        assert_eq!(
            attachment("my report (1).pdf"),
            "attachment; filename=\"my report (1).pdf\""
        );
    }

    #[test]
    fn it_keeps_a_quote_inside_the_quoted_string() {
        assert_eq!(
            attachment("a\"; filename=\"evil.exe"),
            "attachment; filename=\"a\\\"; filename=\\\"evil.exe\"; \
             filename*=UTF-8''a%22%3B%20filename%3D%22evil.exe"
        );
    }

    #[test]
    fn it_escapes_a_backslash() {
        assert_eq!(
            attachment("a\\b.txt"),
            "attachment; filename=\"a\\\\b.txt\"; filename*=UTF-8''a%5Cb.txt"
        );
    }

    #[test]
    fn it_adds_an_ext_value_for_a_non_ascii_name() {
        assert_eq!(
            attachment("résumé.pdf"),
            "attachment; filename=\"r_sum_.pdf\"; filename*=UTF-8''r%C3%A9sum%C3%A9.pdf"
        );
        assert_eq!(
            attachment("отчёт.txt"),
            "attachment; filename=\"_____.txt\"; \
             filename*=UTF-8''%D0%BE%D1%82%D1%87%D1%91%D1%82.txt"
        );
        assert_eq!(
            attachment("🦀.rs"),
            "attachment; filename=\"_.rs\"; filename*=UTF-8''%F0%9F%A6%80.rs"
        );
    }

    #[test]
    fn it_replaces_control_characters() {
        assert_eq!(
            attachment("a\r\nSet-Cookie: x.txt"),
            "attachment; filename=\"a__Set-Cookie: x.txt\""
        );
        assert_eq!(
            attachment("a\0b\tc\x7f.txt"),
            "attachment; filename=\"a_b_c_.txt\""
        );
        assert_eq!(
            attachment("é\u{85}.txt"),
            "attachment; filename=\"__.txt\"; filename*=UTF-8''%C3%A9_.txt"
        );
    }

    #[test]
    fn it_percent_encodes_everything_but_attr_char() {
        assert_eq!(
            attachment("é !#$&+-.^_`|~%'*;=/a1"),
            "attachment; filename=\"_ !#$&+-.^_`|~%'*;=/a1\"; \
             filename*=UTF-8''%C3%A9%20!#$&+-.^_`|~%25%27%2A%3B%3D%2Fa1"
        );
    }

    #[test]
    fn it_accepts_an_empty_name() {
        assert_eq!(attachment(""), "attachment; filename=\"\"");
    }

    #[cfg(feature = "multipart")]
    mod form_data {
        use super::ContentDisposition;

        #[test]
        fn it_writes_name_and_filename() {
            let cd = ContentDisposition::form_data("doc", Some("report.pdf")).unwrap();
            assert_eq!(
                cd.as_ref(),
                "form-data; name=\"doc\"; filename=\"report.pdf\""
            );

            let cd = ContentDisposition::form_data("doc", None).unwrap();
            assert_eq!(cd.as_ref(), "form-data; name=\"doc\"");
        }

        #[test]
        fn it_escapes_quote_and_backslash() {
            let cd = ContentDisposition::form_data("a\"b", Some("c\\d\".txt")).unwrap();
            assert_eq!(
                cd.as_ref(),
                "form-data; name=\"a\\\"b\"; filename=\"c\\\\d\\\".txt\""
            );
        }

        #[test]
        fn it_keeps_utf8_without_an_ext_value() {
            let cd = ContentDisposition::form_data("f", Some("résumé.pdf")).unwrap();
            assert_eq!(
                cd.as_bytes(),
                "form-data; name=\"f\"; filename=\"résumé.pdf\"".as_bytes()
            );
        }

        #[test]
        fn it_rejects_cr_lf() {
            assert!(ContentDisposition::form_data("a\r\nb", None).is_err());
            assert!(ContentDisposition::form_data("a", Some("b\n.txt")).is_err());
        }
    }
}
