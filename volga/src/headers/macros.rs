//! Helper macros for HTTP headers

/// Declares a custom HTTP headers structure
///
/// # Example
/// ```rust
/// use volga::headers::custom_headers;
///
/// // The `x-api-key` header
/// custom_headers! {
///     (ApiKey, "x-api-key")
/// }
/// ```
#[macro_export]
macro_rules! custom_headers {
    ($(($struct_name:ident, $header_name:expr)),* $(,)?) => {
        $(
            /// Custom HTTP header
            #[derive(Debug, Clone, Copy, Eq, PartialEq, PartialOrd, Ord, Hash)]
            pub struct $struct_name;

            impl $crate::headers::FromHeaders for $struct_name {
                const NAME: $crate::headers::HeaderName = $crate::headers::HeaderName::from_static($header_name);
                
                #[inline]
                fn from_headers(headers: &$crate::headers::HeaderMap) -> Option<&$crate::headers::HeaderValue> {
                    headers.get($header_name)
                }
            }
        )*
    };
}

pub use custom_headers;

#[cfg(test)]
#[allow(unreachable_pub)]
mod test {
    use hyper::header::HeaderValue;
    use hyper::HeaderMap;
    use crate::headers::{Header, FromHeaders};

    custom_headers! {
        (ApiKey, "x-api-key")
    }
    
    #[test]
    fn it_creates_custom_headers() {
        let api_key = HeaderValue::from_str("some-api-key").unwrap();
        let api_key_header = Header::<ApiKey>::new(&api_key);
        
        assert_eq!(api_key_header.value(), "some-api-key");
        assert_eq!(ApiKey::NAME, "x-api-key");
    }

    #[test]
    fn it_gets_custom_headers_from_map() {
        let api_key = HeaderValue::from_str("some-api-key").unwrap();
        
        let mut map = HeaderMap::new();
        map.insert("x-api-key", api_key);
        
        let api_key_header = Header::<ApiKey>::from_headers_map(&map).unwrap();

        assert_eq!(api_key_header.as_ref(), "some-api-key");
        assert_eq!(ApiKey::NAME, "x-api-key");
    }
}