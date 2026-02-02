//! Helper macros for HTTP headers

/// Declares a custom HTTP headers structure
///
/// # Example
/// ```rust
/// use volga::headers::headers;
///
/// // The `x-api-key` header
/// headers! {
///     (ApiKey, "x-api-key")
/// }
/// ```
#[macro_export]
macro_rules! headers {
    ($(($struct_name:ident, $header_name:expr)),* $(,)?) => {
        $(
            /// Custom HTTP header
            #[derive(Debug, Clone, Copy, Eq, PartialEq, PartialOrd, Ord, Hash)]
            pub struct $struct_name;

            impl $struct_name {
                /// Creates a new instance of [`Header<T>`] from a `static str`
                ///
                /// # Examples
                /// ```no_run
                /// use volga::headers::ContentType;
                ///
                /// let content_type_header = ContentType::from_static("text/plain");
                /// assert_eq!(content_type_header.as_ref(), "text/plain");
                /// ```
                #[inline(always)]
                pub const fn from_static(value: &'static str) -> $crate::headers::Header<$struct_name> {
                    $crate::headers::Header::<$struct_name>::from_static(value)
                }
            }

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

pub use headers;

#[cfg(test)]
#[allow(unreachable_pub)]
mod test {
    use hyper::header::HeaderValue;
    use hyper::HeaderMap;
    use crate::headers::{Header, FromHeaders};

    headers! {
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