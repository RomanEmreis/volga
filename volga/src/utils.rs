//! Utilities and reusable helpers

pub mod str;

#[cfg(any(feature = "openapi", feature = "static-files"))]
/// Encodes bytes as lowercase hexadecimal.
pub(crate) fn lower_hex(bytes: &[u8]) -> String {
    const HEX: &[u8; 16] = b"0123456789abcdef";

    let mut out = String::with_capacity(bytes.len() * 2);
    for &byte in bytes {
        out.push(HEX[(byte >> 4) as usize] as char);
        out.push(HEX[(byte & 0x0f) as usize] as char);
    }

    out
}

#[cfg(test)]
mod tests {
    use super::lower_hex;

    #[test]
    fn it_encodes_empty_bytes_as_empty_hex() {
        assert_eq!(lower_hex(&[]), "");
    }

    #[test]
    fn it_encodes_ascii_bytes_as_lower_hex() {
        assert_eq!(lower_hex(b"Volga"), "566f6c6761");
    }

    #[test]
    fn it_encodes_mixed_bytes_as_lower_hex() {
        assert_eq!(lower_hex(&[0x00, 0x0f, 0x10, 0xab, 0xff]), "000f10abff");
    }
}
