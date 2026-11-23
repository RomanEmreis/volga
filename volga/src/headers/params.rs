//! Parameters of a header value (RFC 9110 Section 5.6.6): `; name=value`, where the value
//! is a token or a quoted-string (RFC 9110 Section 5.6.4)
//!
//! The one place in volga that writes a quoted-string and reads parameters back, for
//! `Content-Disposition` and the `boundary` of a multipart `Content-Type`.

use std::borrow::Cow;

/// Optional whitespace (RFC 9110 Section 5.6.3)
const OWS: [char; 2] = [' ', '\t'];

/// Writes `chars` as a quoted-string, escaping `"` and `\`.
pub(crate) fn push_quoted(out: &mut String, chars: impl Iterator<Item = char>) {
    out.push('"');
    for ch in chars {
        if is_escaped(ch) {
            out.push('\\');
        }
        out.push(ch);
    }
    out.push('"');
}

/// The length [`push_quoted`] writes for `chars`.
#[inline]
pub(crate) fn quoted_len(chars: impl Iterator<Item = char>) -> usize {
    2 + chars
        .map(|ch| usize::from(is_escaped(ch)) + ch.len_utf8())
        .sum::<usize>()
}

/// Whether a quoted-string escapes this character.
#[inline]
pub(crate) fn is_escaped(ch: char) -> bool {
    matches!(ch, '"' | '\\')
}

/// Iterates over the `name=value` parameters of a header value, separated by `;`.
///
/// A bare token with no `=`, such as the type of a `Content-Disposition`, is skipped.
/// Names are trimmed but keep their case. Iteration ends at a quoted-string that never
/// closes or that is followed by anything but `;`. Nothing is allocated.
#[inline]
pub(crate) fn params(value: &str) -> impl Iterator<Item = (&str, Param<'_>)> {
    let mut rest = value;
    std::iter::from_fn(move || {
        let (name, param, tail) = next_param(rest)?;
        rest = tail;
        Some((name, param))
    })
}

/// Reads the next parameter of `rest`, returning it and what follows it.
fn next_param(mut rest: &str) -> Option<(&str, Param<'_>, &str)> {
    loop {
        rest = rest.trim_start_matches([' ', '\t', ';']);
        let end = rest.find(['=', ';'])?;
        if rest.as_bytes()[end] == b';' {
            // A bare token
            rest = &rest[end..];
            continue;
        }

        let name = rest[..end].trim();
        let value = rest[end + 1..].trim_start_matches(OWS);

        return match value.strip_prefix('"') {
            Some(quoted) => {
                let close = closing_quote(quoted)?;
                let tail = quoted[close + 1..].trim_start_matches(OWS);
                (tail.is_empty() || tail.starts_with(';')).then_some((
                    name,
                    Param::Quoted(&quoted[..close]),
                    tail,
                ))
            }
            None => {
                let end = value.find(';').unwrap_or(value.len());
                let token = value[..end].trim_end_matches(OWS);
                Some((name, Param::Token(token), &value[end..]))
            }
        };
    }
}

/// The index of the `"` that closes a quoted-string whose opening `"` is consumed, past
/// any `\"`. `"` and `\` are ASCII, so a byte scan never stops inside a UTF-8 character.
fn closing_quote(quoted: &str) -> Option<usize> {
    let bytes = quoted.as_bytes();
    let mut i = 0;
    while let Some(offset) = memchr::memchr2(b'"', b'\\', &bytes[i..]) {
        i += offset;
        if bytes[i] == b'"' {
            return Some(i);
        }
        i += 2;
        if i > bytes.len() {
            return None;
        }
    }
    None
}

/// The value of a parameter, as it stands in the header.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Param<'a> {
    /// A token
    Token(&'a str),
    /// The inside of a quoted-string, its escapes still in place
    Quoted(&'a str),
}

impl<'a> Param<'a> {
    /// The value, with the escapes of a quoted-string undone: borrowed unless it has any.
    pub(crate) fn value(self) -> Cow<'a, str> {
        match self {
            Self::Token(token) => Cow::Borrowed(token),
            Self::Quoted(quoted) if !quoted.contains('\\') => Cow::Borrowed(quoted),
            Self::Quoted(quoted) => {
                let mut out = String::with_capacity(quoted.len());
                let mut chars = quoted.chars();
                while let Some(ch) = chars.next() {
                    out.push(match ch {
                        '\\' => chars.next().unwrap_or(ch),
                        ch => ch,
                    });
                }
                Cow::Owned(out)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Param, params, push_quoted, quoted_len};
    use std::borrow::Cow;

    fn collect(value: &str) -> Vec<(&str, Param<'_>)> {
        params(value).collect()
    }

    #[test]
    fn it_quotes_and_measures_the_same_length() {
        for value in ["", "plain", "a\"b", "a\\b", "\"\\\"", "résumé 🦀"] {
            let mut out = String::new();
            push_quoted(&mut out, value.chars());
            assert_eq!(out.len(), quoted_len(value.chars()), "{value}");
        }

        let mut out = String::new();
        push_quoted(&mut out, r#"a"b\c"#.chars());
        assert_eq!(out, r#""a\"b\\c""#);
    }

    #[test]
    fn it_reads_tokens_and_quoted_strings() {
        assert_eq!(
            collect("a=1; b=\"x y\";c = 2 ;d=\"e;f\""),
            [
                ("a", Param::Token("1")),
                ("b", Param::Quoted("x y")),
                ("c", Param::Token("2")),
                ("d", Param::Quoted("e;f")),
            ]
        );
    }

    #[test]
    fn it_skips_bare_tokens() {
        assert_eq!(
            collect("attachment; x; filename=a.txt"),
            [("filename", Param::Token("a.txt"))]
        );
    }

    #[test]
    fn it_keeps_escapes_until_the_value_is_read() {
        let found = collect(r#"a="x\"; b=\"y"; b=z"#);
        assert_eq!(
            found,
            [
                ("a", Param::Quoted(r#"x\"; b=\"y"#)),
                ("b", Param::Token("z"))
            ]
        );
        assert_eq!(found[0].1.value(), r#"x"; b="y"#);
    }

    #[test]
    fn it_borrows_a_value_with_nothing_to_undo() {
        assert!(matches!(Param::Token("a").value(), Cow::Borrowed("a")));
        assert!(matches!(Param::Quoted("a b").value(), Cow::Borrowed("a b")));
        assert!(matches!(Param::Quoted(r"a\\b").value(), Cow::Owned(v) if v == r"a\b"));
    }

    #[test]
    fn it_stops_at_a_malformed_quoted_string() {
        assert_eq!(
            collect("a=1; b=\"never closed; c=2"),
            [("a", Param::Token("1"))]
        );
        assert_eq!(
            collect("a=1; b=\"x\" junk; c=2"),
            [("a", Param::Token("1"))]
        );
        assert_eq!(collect("b=\"ends in a backslash\\"), []);
    }

    #[test]
    fn it_finds_nothing_without_a_parameter() {
        assert_eq!(collect(""), []);
        assert_eq!(collect("attachment"), []);
        assert_eq!(collect("attachment; filename"), []);
    }
}
