use super::{
    Error,
    HeaderValue,
    quality::Ranked
};

use std::{
    str::FromStr,
    fmt
};

#[derive(Hash, Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Encoding {
    Any,
    Identity,
    #[cfg(any(
        feature = "compression-brotli", 
        feature = "decompression-brotli"
    ))]
    Brotli,
    #[cfg(any(
        feature = "compression-gzip",
        feature = "decompression-gzip"
    ))]
    Gzip,
    #[cfg(any(
        feature = "compression-gzip",
        feature = "decompression-gzip"
    ))]
    Deflate,
    #[cfg(any(
        feature = "compression-zstd",
        feature = "decompression-zstd"
    ))]
    Zstd
}

impl Encoding {
    /// Returns `true` is encoding is `*` (star)
    #[inline]
    #[allow(dead_code)]
    pub(crate) fn is_any(&self) -> bool {
        self == &Encoding::Any
    }
    
    /// Creates a comma-separated string of encodings from given list
    #[inline]
    #[allow(dead_code)]
    pub(crate) fn stringify(encoding_list: &[Encoding]) -> String {
        encoding_list.iter()
            .map(|&encoding| encoding.to_string())
            .collect::<Vec<String>>()
            .join(",")
    }
}

impl Ranked for Encoding {
    #[inline]
    fn rank(&self) -> u8 {
        match self {
            #[cfg(any(
                feature = "compression-brotli",
                feature = "decompression-brotli"
            ))]
            Encoding::Brotli => 5,
            #[cfg(any(
                feature = "compression-zstd",
                feature = "decompression-zstd"
            ))]
            Encoding::Zstd => 4,
            #[cfg(any(
                feature = "compression-gzip",
                feature = "decompression-gzip"
            ))]
            Encoding::Gzip => 3,
            #[cfg(any(
                feature = "compression-gzip",
                feature = "decompression-gzip"
            ))]
            Encoding::Deflate => 2,
            Encoding::Any => 1,
            Encoding::Identity => 0,
        }
    }
}

impl FromStr for Encoding {
    type Err = Error;
    
    #[inline]
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s {
            "*" => Ok(Encoding::Any),
            "identity" => Ok(Encoding::Identity),
            #[cfg(any(
                feature = "compression-brotli",
                feature = "decompression-brotli"
            ))]
            "br" => Ok(Encoding::Brotli),
            #[cfg(any(
                feature = "compression-gzip",
                feature = "decompression-gzip"
            ))]
            "gzip" => Ok(Encoding::Gzip),
            #[cfg(any(
                feature = "compression-gzip",
                feature = "decompression-gzip"
            ))]
            "deflate" => Ok(Encoding::Deflate),
            #[cfg(any(
                feature = "compression-zstd",
                feature = "decompression-zstd"
            ))]
            "zstd" => Ok(Encoding::Zstd),
            _ => Err(EncodingError::unknown())
        }
    }
}

impl fmt::Display for Encoding {
    #[inline]
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.write_str(match *self { 
            Encoding::Any => "*",
            Encoding::Identity => "identity",
            #[cfg(any(
                feature = "compression-brotli",
                feature = "decompression-brotli"
            ))]
            Encoding::Brotli => "br",
            #[cfg(any(
                feature = "compression-gzip",
                feature = "decompression-gzip"
            ))]
            Encoding::Gzip => "gzip",
            #[cfg(any(
                feature = "compression-gzip",
                feature = "decompression-gzip"
            ))]
            Encoding::Deflate => "deflate",
            #[cfg(any(
                feature = "compression-zstd",
                feature = "decompression-zstd"
            ))]
            Encoding::Zstd => "zstd"
        })
    }
}

impl From<Encoding> for HeaderValue {
    #[inline]
    fn from(encoding: Encoding) -> HeaderValue {
        HeaderValue::from_str(&encoding.to_string())
            .unwrap_or(HeaderValue::from_static("identity"))
    }
}

impl TryFrom<HeaderValue> for Encoding {
    type Error = Error;
    
    fn try_from(header_value: HeaderValue) -> Result<Encoding, Error> {
        if header_value.is_empty() { 
            return Err(EncodingError::empty());
        } 
        
        let val = header_value
            .to_str()
            .map_err(|_| EncodingError::unknown())?;
        Encoding::from_str(val)
    }
}

struct EncodingError;
impl EncodingError {
    fn unknown() -> Error { 
        Error::client_error("Encoding error: Unknown encoding")
    }

    fn empty() -> Error {
        Error::client_error("Encoding error: Empty encoding")
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;
    use crate::headers::HeaderValue;
    use crate::headers::quality::Ranked;
    use super::Encoding;
    
    #[test]
    fn it_parses_from_str() {
        let encodings = [
            ("*", Encoding::Any),
            ("identity", Encoding::Identity),
            #[cfg(any(feature = "compression-brotli", feature = "decompression-brotli"))]
            ("br", Encoding::Brotli),
            #[cfg(any(feature = "compression-gzip", feature = "decompression-gzip"))]
            ("gzip", Encoding::Gzip),
            #[cfg(any(feature = "compression-gzip", feature = "decompression-gzip"))]
            ("deflate", Encoding::Deflate),
            #[cfg(any(feature = "compression-zstd", feature = "decompression-zstd"))]
            ("zstd", Encoding::Zstd),
        ];
        
        for (encoding_str, encoding) in encodings {
            assert_eq!(Encoding::from_str(encoding_str).unwrap(), encoding);
        }
    }
    
    #[test]
    fn it_converts_to_str() {
        let encodings = [
            ("*", Encoding::Any),
            ("identity", Encoding::Identity),
            #[cfg(any(feature = "compression-brotli", feature = "decompression-brotli"))]
            ("br", Encoding::Brotli),
            #[cfg(any(feature = "compression-gzip", feature = "decompression-gzip"))]
            ("gzip", Encoding::Gzip),
            #[cfg(any(feature = "compression-gzip", feature = "decompression-gzip"))]
            ("deflate", Encoding::Deflate),
            #[cfg(any(feature = "compression-zstd", feature = "decompression-zstd"))]
            ("zstd", Encoding::Zstd),
        ];

        for (encoding_str, encoding) in encodings {
            assert_eq!(encoding.to_string(), encoding_str);
        }
    }
    
    #[test]
    fn it_returns_error() {
        let encoding_str = "abc";
        
        let encoding = Encoding::from_str(encoding_str);
        
        assert!(encoding.is_err());
    }
    
    #[test]
    fn it_returns_true_for_any_encoding() {
        assert!(Encoding::Any.is_any());
    }

    #[test]
    fn it_returns_false_for_other_encodings() {
        let encodings = [
            Encoding::Identity,
            #[cfg(any(feature = "compression-brotli", feature = "decompression-brotli"))]
            Encoding::Brotli,
            #[cfg(any(feature = "compression-gzip", feature = "decompression-gzip"))]
            Encoding::Gzip,
            #[cfg(any(feature = "compression-gzip", feature = "decompression-gzip"))]
            Encoding::Deflate,
            #[cfg(any(feature = "compression-zstd", feature = "decompression-zstd"))]
            Encoding::Zstd
        ];

        for encoding in &encodings {
            assert!(!encoding.is_any());
        }
    }
    
    #[test]
    fn it_converts_to_header_value() {
        let encodings = [
            Encoding::Any,
            Encoding::Identity,
            #[cfg(any(feature = "compression-brotli", feature = "decompression-brotli"))]
            Encoding::Brotli,
            #[cfg(any(feature = "compression-gzip", feature = "decompression-gzip"))]
            Encoding::Gzip,
            #[cfg(any(feature = "compression-gzip", feature = "decompression-gzip"))]
            Encoding::Deflate,
            #[cfg(any(feature = "compression-zstd", feature = "decompression-zstd"))]
            Encoding::Zstd
        ];

        for encoding in encodings {
            assert_eq!(HeaderValue::from(encoding), encoding.to_string());
        }
    }

    #[test]
    fn it_returns_correct_ranks() {
        let encodings = [
            Encoding::Identity,
            Encoding::Any,
            #[cfg(any(feature = "compression-gzip", feature = "decompression-gzip"))]
            Encoding::Deflate,
            #[cfg(any(feature = "compression-gzip", feature = "decompression-gzip"))]
            Encoding::Gzip,
            #[cfg(any(feature = "compression-zstd", feature = "decompression-zstd"))]
            Encoding::Zstd,
            #[cfg(any(feature = "compression-brotli", feature = "decompression-brotli"))]
            Encoding::Brotli,
        ];

        for (i, encoding) in encodings.iter().enumerate() {
            assert_eq!(i, encoding.rank() as usize);
        }
    }
    
    #[test]
    #[cfg(any(feature = "compression-gzip", feature = "decompression-gzip"))]
    fn it_stringifies_list_of_encodings() {
        let encodings = [Encoding::Identity, Encoding::Gzip, Encoding::Deflate];
        let encodings_str = Encoding::stringify(&encodings);
        
        assert_eq!(encodings_str, "identity,gzip,deflate");
    }

    #[test]
    fn it_returns_error_from_header_value() {
        let header = HeaderValue::from_static("abc");

        let encoding = Encoding::try_from(header);

        assert!(encoding.is_err());
    }

    #[test]
    fn it_returns_error_from_empty_header_value() {
        let header = HeaderValue::from_static("");

        let encoding = Encoding::try_from(header);

        assert!(encoding.is_err());
    }
}