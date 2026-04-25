//! PEM header detection for auto-selecting the right JWT key constructor.

/// The key format inferred from a PEM header.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[allow(dead_code)]
pub(crate) enum PemKind {
    /// `-----BEGIN RSA PRIVATE KEY-----` or `-----BEGIN RSA PUBLIC KEY-----`.
    Rsa,
    /// `-----BEGIN EC PRIVATE KEY-----`.
    Ec,
    /// `-----BEGIN PRIVATE KEY-----` or `-----BEGIN PUBLIC KEY-----` — ambiguous
    /// between RSA, EC, and Ed. Caller should try each constructor in order.
    Ambiguous,
    /// Header was not recognized.
    Unknown,
}

/// Inspects the first recognizable PEM header line in `bytes` and returns the
/// inferred key kind. Leading whitespace / blank lines are skipped.
#[allow(dead_code)]
pub(crate) fn detect(bytes: &[u8]) -> PemKind {
    for line in bytes.split(|&b| b == b'\n') {
        let trimmed = trim_ascii(line);
        if trimmed.is_empty() {
            continue;
        }
        return match trimmed {
            b"-----BEGIN RSA PRIVATE KEY-----" | b"-----BEGIN RSA PUBLIC KEY-----" => PemKind::Rsa,
            b"-----BEGIN EC PRIVATE KEY-----" => PemKind::Ec,
            b"-----BEGIN PRIVATE KEY-----" | b"-----BEGIN PUBLIC KEY-----" => PemKind::Ambiguous,
            _ => PemKind::Unknown,
        };
    }
    PemKind::Unknown
}

#[allow(dead_code)]
fn trim_ascii(mut bytes: &[u8]) -> &[u8] {
    while let [first, rest @ ..] = bytes {
        if first.is_ascii_whitespace() {
            bytes = rest;
        } else {
            break;
        }
    }
    while let [rest @ .., last] = bytes {
        if last.is_ascii_whitespace() {
            bytes = rest;
        } else {
            break;
        }
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_detects_rsa_private() {
        assert_eq!(
            detect(b"-----BEGIN RSA PRIVATE KEY-----\nabc\n"),
            PemKind::Rsa
        );
    }

    #[test]
    fn it_detects_rsa_public() {
        assert_eq!(
            detect(b"-----BEGIN RSA PUBLIC KEY-----\nabc\n"),
            PemKind::Rsa
        );
    }

    #[test]
    fn it_detects_ec_private() {
        assert_eq!(
            detect(b"-----BEGIN EC PRIVATE KEY-----\nabc\n"),
            PemKind::Ec
        );
    }

    #[test]
    fn it_detects_pkcs8_as_ambiguous() {
        assert_eq!(
            detect(b"-----BEGIN PRIVATE KEY-----\nabc\n"),
            PemKind::Ambiguous
        );
    }

    #[test]
    fn it_detects_spki_as_ambiguous() {
        assert_eq!(
            detect(b"-----BEGIN PUBLIC KEY-----\nabc\n"),
            PemKind::Ambiguous
        );
    }

    #[test]
    fn it_returns_unknown_for_no_header() {
        assert_eq!(detect(b"no header here"), PemKind::Unknown);
    }

    #[test]
    fn it_returns_unknown_for_unfamiliar_header() {
        assert_eq!(
            detect(b"-----BEGIN CERTIFICATE-----\nabc\n"),
            PemKind::Unknown
        );
    }

    #[test]
    fn it_handles_leading_whitespace() {
        assert_eq!(
            detect(b"\n\n-----BEGIN EC PRIVATE KEY-----\nabc\n"),
            PemKind::Ec
        );
    }

    #[test]
    fn it_returns_unknown_for_empty_input() {
        assert_eq!(detect(b""), PemKind::Unknown);
    }

    #[test]
    fn it_returns_unknown_for_only_whitespace() {
        assert_eq!(detect(b"   \n\t\n   \n"), PemKind::Unknown);
    }

    #[test]
    fn it_handles_crlf_line_endings() {
        assert_eq!(
            detect(b"-----BEGIN RSA PRIVATE KEY-----\r\nabc\r\n"),
            PemKind::Rsa
        );
    }

    #[test]
    fn it_handles_trailing_whitespace_on_header_line() {
        assert_eq!(
            detect(b"-----BEGIN EC PRIVATE KEY-----   \nabc\n"),
            PemKind::Ec
        );
    }

    #[test]
    fn it_detects_header_without_trailing_newline() {
        assert_eq!(detect(b"-----BEGIN RSA PRIVATE KEY-----"), PemKind::Rsa);
    }

    #[test]
    fn it_skips_leading_blank_lines_with_mixed_whitespace() {
        assert_eq!(
            detect(b"\r\n  \t\n-----BEGIN PUBLIC KEY-----\n"),
            PemKind::Ambiguous
        );
    }

    #[test]
    fn it_stops_at_first_non_empty_line() {
        // A garbage first line should yield Unknown even if a valid header
        // appears later — we only inspect the first non-blank line.
        assert_eq!(
            detect(b"garbage\n-----BEGIN RSA PRIVATE KEY-----\n"),
            PemKind::Unknown
        );
    }
}
