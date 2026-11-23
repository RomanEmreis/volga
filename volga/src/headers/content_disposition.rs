//! Builders and the parser of the `Content-Disposition` header (RFC 6266, RFC 8187, RFC 7578)
//!
//! Every value volga builds for this header goes through here, so that a file or a field
//! name, which often comes from user input, cannot end the quoted string it is written in
//! and add parameters of its own; and the file name of a request is read here, with the
//! same quoting undone. The quoting and the parameters themselves are those of
//! [`params`](super::params).
//!
//! A built value is written into a `String` of its exact length, which the header value
//! then takes over without a copy, so building one allocates once. Reading allocates
//! nothing unless the name is escaped or percent-encoded.

use std::borrow::Cow;

use super::params::{Param, is_escaped, params, push_quoted, quoted_len};
use super::{ContentDisposition, Header, HeaderError, HeaderValue};
use crate::{error::Error, http::endpoints::route::percent_decode};

const FILENAME: &str = "; filename=";
const FILENAME_EXT: &str = "; filename*=UTF-8''";

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
        const NAME: &str = "form-data; name=";

        let len = NAME.len()
            + quoted_len(name.chars())
            + filename.map_or(0, |filename| FILENAME.len() + quoted_len(filename.chars()));

        let mut value = String::with_capacity(len);
        value.push_str(NAME);
        push_quoted(&mut value, name.chars());

        if let Some(filename) = filename {
            value.push_str(FILENAME);
            push_quoted(&mut value, filename.chars());
        }

        debug_assert_eq!(value.len(), len);
        into_header(value)
    }
}

/// Builds `<kind>; filename="<ascii fallback>"[; filename*=UTF-8''<percent-encoded>]`.
fn with_filename(kind: &'static str, filename: &str) -> Header<ContentDisposition> {
    // Only a name with control characters, which no honest client sends, is copied
    let filename = if filename.contains(char::is_control) {
        Cow::Owned(filename.replace(char::is_control, "_"))
    } else {
        Cow::Borrowed(filename)
    };
    // The ASCII fallback: no control characters are left, so this is all it replaces
    let fallback = || {
        filename
            .chars()
            .map(|ch| if ch.is_ascii() { ch } else { '_' })
    };
    // The exact name goes in `filename*` too when the fallback loses or escapes some of it
    let ext_value = filename
        .contains(|ch: char| is_escaped(ch) || !ch.is_ascii())
        .then_some(&*filename);

    let len = kind.len()
        + FILENAME.len()
        + quoted_len(fallback())
        + ext_value.map_or(0, |ext| FILENAME_EXT.len() + percent_encoded_len(ext));

    let mut value = String::with_capacity(len);
    value.push_str(kind);
    value.push_str(FILENAME);
    push_quoted(&mut value, fallback());

    if let Some(ext) = ext_value {
        value.push_str(FILENAME_EXT);
        push_percent_encoded(&mut value, ext);
    }

    debug_assert_eq!(value.len(), len);

    // The value holds visible ASCII and spaces alone, which a header value always accepts;
    // the bare disposition type is there so that this never panics.
    into_header(value).unwrap_or_else(|_| ContentDisposition::from_static(kind))
}

/// Writes `value` with every byte but an `attr-char` as `%XX`.
fn push_percent_encoded(out: &mut String, value: &str) {
    const HEX: &[u8; 16] = b"0123456789ABCDEF";

    for byte in value.bytes() {
        if is_attr_char(byte) {
            out.push(char::from(byte));
        } else {
            out.push('%');
            out.push(char::from(HEX[usize::from(byte >> 4)]));
            out.push(char::from(HEX[usize::from(byte & 0x0F)]));
        }
    }
}

/// The length [`push_percent_encoded`] writes for `value`.
#[inline]
fn percent_encoded_len(value: &str) -> usize {
    // Each escape is three bytes in place of one
    value.len() + 2 * value.bytes().filter(|&b| !is_attr_char(b)).count()
}

/// `attr-char` (RFC 8187 Section 3.2.1): the bytes an `ext-value` keeps as they are.
#[inline]
fn is_attr_char(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || b"!#$&+-.^_`|~".contains(&byte)
}

/// Hands the value over to a header value, which takes the buffer without a copy.
#[inline]
fn into_header(value: String) -> Result<Header<ContentDisposition>, Error> {
    HeaderValue::try_from(value)
        .map(Header::new)
        .map_err(HeaderError::from_invalid_header_value)
}

/// Reads the file name from a `Content-Disposition` value: `filename*` (RFC 8187, UTF-8
/// only) when it is there and decodes, `filename` otherwise, with its quoted-string escapes
/// undone. Parameter names are matched case-insensitively.
///
/// The name is borrowed from `value` unless it is escaped or percent-encoded. It is the
/// client's: it is not checked here for path separators or `..`.
pub(crate) fn parse_filename(value: &str) -> Option<Cow<'_, str>> {
    let mut filename = None;
    for (name, param) in params(value) {
        if name.eq_ignore_ascii_case("filename*") {
            // An ext-value is a token; a quoted one is malformed (RFC 8187 Section 3.2.1)
            if let Param::Token(ext) = param
                && let Some(decoded) = decode_ext_value(ext)
            {
                return Some(decoded);
            }
        } else if name.eq_ignore_ascii_case("filename") && filename.is_none() {
            filename = Some(param);
        }
    }
    filename.map(Param::value)
}

/// Decodes an `ext-value` (`UTF-8'<language>'<percent-encoded>`): borrowed unless it holds
/// an escape. Any other charset, or escapes that are malformed or not UTF-8, give `None`.
fn decode_ext_value(value: &str) -> Option<Cow<'_, str>> {
    let mut parts = value.splitn(3, '\'');
    let (charset, _language, encoded) = (parts.next()?, parts.next()?, parts.next()?);
    if !charset.eq_ignore_ascii_case("utf-8") {
        return None;
    }
    percent_decode(encoded).ok()
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

    mod parse {
        use super::super::parse_filename;
        use super::ContentDisposition;

        #[test]
        fn it_reads_a_token_or_a_quoted_filename() {
            assert_eq!(
                parse_filename("attachment; filename=a.txt").as_deref(),
                Some("a.txt")
            );
            assert_eq!(
                parse_filename("attachment;filename=\"my file.txt\"").as_deref(),
                Some("my file.txt")
            );
            assert_eq!(
                parse_filename("form-data; name=\"f\"; FileName = \"a;b.txt\" ").as_deref(),
                Some("a;b.txt")
            );
            assert_eq!(parse_filename("filename=x").as_deref(), Some("x"));
        }

        #[test]
        fn it_undoes_quoted_pairs() {
            assert_eq!(
                parse_filename(r#"attachment; filename="a\"; filename=\"evil.exe""#).as_deref(),
                Some("a\"; filename=\"evil.exe")
            );
            assert_eq!(
                parse_filename(r#"attachment; filename="a\\b""#).as_deref(),
                Some("a\\b")
            );
        }

        #[test]
        fn it_prefers_a_decodable_ext_value() {
            assert_eq!(
                parse_filename(
                    "attachment; filename=\"r_sum_.pdf\"; filename*=UTF-8''r%C3%A9sum%C3%A9.pdf"
                )
                .as_deref(),
                Some("résumé.pdf")
            );
            assert_eq!(
                parse_filename("attachment; filename*=utf-8'en'a%20b.txt; filename=x").as_deref(),
                Some("a b.txt")
            );
            for broken in [
                "attachment; filename=x; filename*=ISO-8859-1''a.txt",
                "attachment; filename=x; filename*=UTF-8''a%ZZ.txt",
                "attachment; filename=x; filename*=UTF-8''a%C3",
                "attachment; filename=x; filename*=UTF-8''%FF",
                "attachment; filename=x; filename*=a.txt",
                "attachment; filename=x; filename*=\"UTF-8''a.txt\"",
            ] {
                assert_eq!(parse_filename(broken).as_deref(), Some("x"), "{broken}");
            }
        }

        #[test]
        fn it_reads_what_attachment_writes() {
            for name in [
                "report.pdf",
                "a\"; filename=\"evil.exe",
                "a\\b",
                "résumé.pdf",
                "🦀 & co.rs",
            ] {
                let header = ContentDisposition::attachment(name);
                assert_eq!(
                    parse_filename(header.as_str().unwrap()).as_deref(),
                    Some(name)
                );
            }
        }

        #[test]
        fn it_borrows_a_name_with_nothing_to_undo() {
            use std::borrow::Cow;

            for value in [
                "attachment; filename=a.txt",
                "attachment; filename=\"a b.txt\"",
                "attachment; filename*=UTF-8''a.txt",
                "form-data; name=\"x\\\"y\"; filename=\"a.txt\"",
            ] {
                assert!(
                    matches!(parse_filename(value), Some(Cow::Borrowed(_))),
                    "{value}"
                );
            }
            for value in [
                "attachment; filename=\"a\\\"b.txt\"",
                "attachment; filename*=UTF-8''a%20b.txt",
            ] {
                assert!(
                    matches!(parse_filename(value), Some(Cow::Owned(_))),
                    "{value}"
                );
            }
        }

        #[test]
        fn it_finds_no_filename() {
            for value in [
                "",
                "attachment",
                "inline; name=x",
                "attachment; filename=\"never closed",
                "attachment; filename",
                "attachment; filename=\"ends in a backslash\\",
            ] {
                assert_eq!(parse_filename(value), None, "{value}");
            }
        }
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
