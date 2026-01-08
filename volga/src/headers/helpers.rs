use std::time::SystemTime;
use httpdate::parse_http_date;

use crate::headers::{
    ETag,
    HttpHeaders,
    IF_MODIFIED_SINCE,
    IF_NONE_MATCH
};

#[inline]
#[allow(dead_code)]
pub(crate) fn validate_etag(etag: &ETag, headers: &HttpHeaders) -> bool {
    headers.get_raw(&IF_NONE_MATCH)
        .and_then(|if_none_match| if_none_match.to_str().ok())
        .is_some_and(|value| value.split(',').any(|v| v.trim() == etag.as_ref()))
}

#[inline]
#[allow(dead_code)]
pub(crate) fn validate_last_modified(last_modified: SystemTime, headers: &HttpHeaders) -> bool {
    headers.get_raw(&IF_MODIFIED_SINCE)
        .and_then(|if_modified_since| if_modified_since.to_str().ok())
        .and_then(|if_modified_since| parse_http_date(if_modified_since).ok())
        .is_some_and(|value| last_modified <= value)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;
    use super::*;
    use crate::headers::{
        ETag,
        HeaderMap, HeaderValue, HttpHeaders,
        IF_NONE_MATCH, IF_MODIFIED_SINCE
    };

    #[test]
    fn it_validates_etag_list() {
        let mut headers = HeaderMap::new();
        headers.insert(IF_NONE_MATCH, HeaderValue::from_static("\"123\",\"321\",\"111\""));
        
        let headers = HttpHeaders::from(headers);
        
        assert!(validate_etag(&ETag::new("123"), &headers));
    }

    #[test]
    fn it_validates_etag_not_present_in_list() {
        let mut headers = HeaderMap::new();
        headers.insert(IF_NONE_MATCH, HeaderValue::from_static("\"123\",\"321\",\"111\""));

        let headers = HttpHeaders::from(headers);

        assert!(!validate_etag(&ETag::new("555"), &headers));
    }

    #[test]
    fn it_validates_etag_single() {
        let mut headers = HeaderMap::new();
        headers.insert(IF_NONE_MATCH, HeaderValue::from_static("\"123\""));

        let headers = HttpHeaders::from(headers);

        assert!(validate_etag(&ETag::new("123"), &headers));
    }

    #[test]
    fn it_validates_etag_single_different_value() {
        let mut headers = HeaderMap::new();
        headers.insert(IF_NONE_MATCH, HeaderValue::from_static("\"123\""));

        let headers = HttpHeaders::from(headers);

        assert!(!validate_etag(&ETag::new("555"), &headers));
    }

    #[test]
    fn it_validates_etag_when_if_none_match_missing() {
        let headers = HttpHeaders::from(HeaderMap::new());

        assert!(!validate_etag(&ETag::new("123"), &headers));
    }
    
    #[test]
    fn it_validates_last_modified() {
        let now = SystemTime::now();
        let mut headers = HeaderMap::new();
        headers.insert(IF_MODIFIED_SINCE, HeaderValue::from_str(&httpdate::fmt_http_date(now)).unwrap());

        let headers = HttpHeaders::from(headers);

        assert!(validate_last_modified(now - Duration::from_secs(10), &headers));
    }

    #[test]
    fn it_validates_last_modified_resource_has_been_updated() {
        let now = SystemTime::now();
        let mut headers = HeaderMap::new();
        headers.insert(IF_MODIFIED_SINCE, HeaderValue::from_str(&httpdate::fmt_http_date(now)).unwrap());

        let headers = HttpHeaders::from(headers);

        assert!(!validate_last_modified(now + Duration::from_secs(10), &headers));
    }

    #[test]
    fn it_validates_last_modified_when_if_modified_since_missing() {
        let now = SystemTime::now();
        let headers = HttpHeaders::from(HeaderMap::new());

        assert!(!validate_last_modified(now, &headers));
    }
}