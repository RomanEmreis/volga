//! Extractors for default HTTP headers

use super::FromHeaders;
use super::{X_ACCEL_BUFFERING, X_FORWARDED_FOR};
use hyper::header::{
    ACCEPT, ACCEPT_CHARSET, ACCEPT_ENCODING, ACCEPT_LANGUAGE, ACCEPT_RANGES,
    ACCESS_CONTROL_ALLOW_CREDENTIALS, ACCESS_CONTROL_ALLOW_HEADERS, ACCESS_CONTROL_ALLOW_METHODS,
    ACCESS_CONTROL_ALLOW_ORIGIN, ACCESS_CONTROL_EXPOSE_HEADERS, ACCESS_CONTROL_MAX_AGE,
    ACCESS_CONTROL_REQUEST_HEADERS, ACCESS_CONTROL_REQUEST_METHOD, AGE, ALLOW, ALT_SVC,
    AUTHORIZATION, CACHE_CONTROL, CONNECTION, CONTENT_DISPOSITION, CONTENT_ENCODING,
    CONTENT_LANGUAGE, CONTENT_LENGTH, CONTENT_LOCATION, CONTENT_RANGE, CONTENT_SECURITY_POLICY,
    CONTENT_SECURITY_POLICY_REPORT_ONLY, CONTENT_TYPE, COOKIE, DATE, DNT, ETAG, EXPECT, EXPIRES,
    FORWARDED, FROM, HOST, IF_MATCH, IF_MODIFIED_SINCE, IF_NONE_MATCH, IF_RANGE,
    IF_UNMODIFIED_SINCE, LAST_MODIFIED, LINK, LOCATION, MAX_FORWARDS, ORIGIN, PRAGMA,
    PROXY_AUTHENTICATE, PROXY_AUTHORIZATION, PUBLIC_KEY_PINS, PUBLIC_KEY_PINS_REPORT_ONLY, RANGE,
    REFERER, REFERRER_POLICY, REFRESH, RETRY_AFTER, SEC_WEBSOCKET_ACCEPT, SEC_WEBSOCKET_EXTENSIONS,
    SEC_WEBSOCKET_KEY, SEC_WEBSOCKET_PROTOCOL, SEC_WEBSOCKET_VERSION, SERVER, SET_COOKIE,
    STRICT_TRANSPORT_SECURITY, TE, TRAILER, TRANSFER_ENCODING, UPGRADE, UPGRADE_INSECURE_REQUESTS,
    USER_AGENT, VARY, VIA, WARNING, WWW_AUTHENTICATE, X_CONTENT_TYPE_OPTIONS,
    X_DNS_PREFETCH_CONTROL, X_FRAME_OPTIONS, X_XSS_PROTECTION
};

macro_rules! define_header {
    ($(($struct_name:ident, $header_name:ident)),* $(,)?) => {
        $(
            #[doc = concat!("See [`", stringify!($header_name), "`] for more details.")]
            #[allow(missing_debug_implementations)]
            #[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
            pub struct $struct_name;

            impl $struct_name {
                /// Creates a new instance of [`Header<T>`] from a `static str`
                #[inline(always)]
                pub const fn from_static(value: &'static str) -> $crate::headers::Header<$struct_name> {
                    $crate::headers::Header::<$struct_name>::from_static(value)
                }
                
                /// Construct a typed header from bytes (validated).
                #[inline]
                pub fn from_bytes(bytes: &[u8]) -> Result<$crate::headers::Header<$struct_name>, $crate::error::Error> {
                    $crate::headers::Header::<$struct_name>::from_bytes(bytes)
                }

                /// Wrap an owned raw HeaderValue (validated elsewhere).
                #[inline]
                pub fn new(value: $crate::headers::HeaderValue) -> $crate::headers::Header<$struct_name> {
                    $crate::headers::Header::<$struct_name>::new(value)
                }

                /// Wrap a borrowed raw HeaderValue (validated elsewhere).
                #[inline]
                pub fn from_ref(value: &$crate::headers::HeaderValue) -> $crate::headers::Header<$struct_name> {
                    $crate::headers::Header::<$struct_name>::from_ref(value)
                }
            }

            impl FromHeaders for $struct_name {
                const NAME: $crate::headers::HeaderName = $header_name;
                
                #[inline]
                fn from_headers(headers: &$crate::headers::HeaderMap) -> Option<&$crate::headers::HeaderValue> {
                    headers.get($header_name)
                }
            }
        )*
    };
}

define_header! {
    (Accept, ACCEPT), (AcceptCharset, ACCEPT_CHARSET), (AcceptEncoding, ACCEPT_ENCODING), (AcceptLanguage, ACCEPT_LANGUAGE), (AcceptRanges, ACCEPT_RANGES),
    (AccessControlAllowCredentials, ACCESS_CONTROL_ALLOW_CREDENTIALS), (AccessControlAllowHeaders, ACCESS_CONTROL_ALLOW_HEADERS),
    (AccessControlAllowMethods, ACCESS_CONTROL_ALLOW_METHODS), (AccessControlAllowOrigin, ACCESS_CONTROL_ALLOW_ORIGIN),
    (AccessControlAllowExposeHeaders, ACCESS_CONTROL_EXPOSE_HEADERS), (AccessControlAllowMaxAge, ACCESS_CONTROL_MAX_AGE),
    (AccessControlRequestHeaders, ACCESS_CONTROL_REQUEST_HEADERS), (AccessControlRequestMethod, ACCESS_CONTROL_REQUEST_METHOD), (Age, AGE), (Allow, ALLOW), (AltSvc, ALT_SVC),
    (Authorization, AUTHORIZATION), (CacheControl, CACHE_CONTROL), (Connection, CONNECTION), (ContentDisposition, CONTENT_DISPOSITION), (ContentEncoding, CONTENT_ENCODING),
    (ContentLanguage, CONTENT_LANGUAGE), (ContentLength, CONTENT_LENGTH), (ContentLocation, CONTENT_LOCATION), (ContentRange, CONTENT_RANGE), (ContentSecurityPolicy, CONTENT_SECURITY_POLICY),
    (ContentSecurityPolicyReportOnly, CONTENT_SECURITY_POLICY_REPORT_ONLY), (ContentType, CONTENT_TYPE), (Cookie, COOKIE), (Date, DATE), (Dnt, DNT), (Etag, ETAG), (Expect, EXPECT), (Expires, EXPIRES),
    (Forwarded, FORWARDED), (From, FROM), (Host, HOST), (IfMatch, IF_MATCH), (IfModifiedSince, IF_MODIFIED_SINCE), (IfNoneMatch, IF_NONE_MATCH), (IfRange, IF_RANGE),
    (IfUnmodifiedSince, IF_UNMODIFIED_SINCE), (LastModified, LAST_MODIFIED), (Link, LINK), (Location, LOCATION), (MaxForwards, MAX_FORWARDS), (Origin, ORIGIN), (Pragma, PRAGMA),
    (ProxyAuthenticate, PROXY_AUTHENTICATE), (ProxyAuthorization, PROXY_AUTHORIZATION), (PublicKeyPins, PUBLIC_KEY_PINS), (PublicKeyPinsReportOnly, PUBLIC_KEY_PINS_REPORT_ONLY), (Range, RANGE),
    (Referer, REFERER), (ReferrerPolicy, REFERRER_POLICY), (Refresh, REFRESH), (RetryAfter, RETRY_AFTER), (SecWebSocketAccept, SEC_WEBSOCKET_ACCEPT), (SecWebSocketExtensions, SEC_WEBSOCKET_EXTENSIONS),
    (SecWebSocketKey, SEC_WEBSOCKET_KEY), (SecWebSocketProtocol, SEC_WEBSOCKET_PROTOCOL), (SecWebSocketVersion, SEC_WEBSOCKET_VERSION), (Server, SERVER), (SetCookie, SET_COOKIE),
    (StrictTransportSecurity, STRICT_TRANSPORT_SECURITY), (Te, TE), (Trailer, TRAILER), (TransferEncoding, TRANSFER_ENCODING), (Upgrade, UPGRADE), (UpgradeInsecureRequests, UPGRADE_INSECURE_REQUESTS),
    (UserAgent, USER_AGENT), (Vary, VARY), (Via, VIA), (Warning, WARNING), (WwwAuthenticate, WWW_AUTHENTICATE), (XContentTypeOptions, X_CONTENT_TYPE_OPTIONS),
    (XDnsPrefetchControl, X_DNS_PREFETCH_CONTROL), (XFrameOptions, X_FRAME_OPTIONS), (XXssProtection, X_XSS_PROTECTION), (XAccelBuffering, X_ACCEL_BUFFERING), (XForwardedFor, X_FORWARDED_FOR)
}

#[cfg(test)]
mod tests {
    use super::{ContentType, Host};
    use crate::headers::{FromHeaders, HeaderMap, HeaderValue, HOST, CONTENT_TYPE};

    #[test]
    fn it_extracts_headers_from_map() {
        let mut headers = HeaderMap::new();
        headers.insert(HOST, HeaderValue::from_static("example.com"));
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));

        assert_eq!(
            Host::from_headers(&headers).unwrap(),
            &HeaderValue::from_static("example.com")
        );
        assert_eq!(
            ContentType::from_headers(&headers).unwrap(),
            &HeaderValue::from_static("application/json")
        );
    }

    #[test]
    fn it_returns_none_when_header_missing() {
        let headers = HeaderMap::new();
        assert!(Host::from_headers(&headers).is_none());
        assert!(ContentType::from_headers(&headers).is_none());
    }

    #[test]
    fn it_creates_header_from_static() {
        assert_eq!(
            ContentType::from_static("text/plain").as_ref(),
            &HeaderValue::from_static("text/plain")
        );
    }
}