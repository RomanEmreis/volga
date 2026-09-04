use httpdate::parse_http_date;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use crate::headers::{ETag, ETagRef, HttpHeaders, IF_MODIFIED_SINCE, IF_NONE_MATCH};

#[inline]
#[allow(dead_code)]
pub(crate) fn validate_etag(etag: &ETag, headers: &HttpHeaders) -> bool {
    let Some(hv) = headers.get_raw(&IF_NONE_MATCH) else {
        return false;
    };

    let Ok(s) = hv.to_str() else {
        return false;
    };

    let target_tag = etag.tag();

    s.split(',')
        .map(|v| v.trim())
        .filter(|v| !v.is_empty())
        .filter_map(|raw| ETagRef::parse(raw).ok())
        .any(|candidate| candidate.weak_eq_tag(target_tag))
}

#[inline]
#[allow(dead_code)]
pub(crate) fn validate_last_modified(last_modified: SystemTime, headers: &HttpHeaders) -> bool {
    headers
        .get_raw(&IF_MODIFIED_SINCE)
        .and_then(|if_modified_since| if_modified_since.to_str().ok())
        .and_then(|if_modified_since| parse_http_date(if_modified_since).ok())
        .is_some_and(|value| truncate_to_secs(last_modified) <= value)
}

/// Drops the sub-second part of a timestamp.
///
/// An HTTP-date carries whole seconds (RFC 9110 Section 5.6.7), so that is the precision the
/// client echoes back in `If-Modified-Since` - while a modern filesystem reports an `mtime`
/// with nanoseconds. Comparing the two as they are makes any file whose `mtime` has a
/// fractional part look strictly newer than the very `Last-Modified` it was just served
/// with, and it would never validate.
#[inline]
fn truncate_to_secs(time: SystemTime) -> SystemTime {
    match time.duration_since(UNIX_EPOCH) {
        Ok(since_epoch) => UNIX_EPOCH + Duration::from_secs(since_epoch.as_secs()),
        // Older than the epoch, so out of range for an HTTP-date anyway.
        Err(_) => time,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::headers::{
        ETag, HeaderMap, HeaderValue, HttpHeaders, IF_MODIFIED_SINCE, IF_NONE_MATCH,
    };
    use std::time::Duration;

    #[test]
    fn it_validates_a_last_modified_it_just_emitted() {
        // A file whose mtime carries nanoseconds - the norm on ext4, APFS and NTFS.
        let modified = UNIX_EPOCH + Duration::from_nanos(1_700_000_000_721_955_581);

        // The client echoes back exactly what `Last-Modified` carried, whole seconds.
        let mut headers = HeaderMap::new();
        headers.insert(
            IF_MODIFIED_SINCE,
            HeaderValue::from_str(&httpdate::fmt_http_date(modified)).unwrap(),
        );

        assert!(validate_last_modified(
            modified,
            &HttpHeaders::from(headers)
        ));
    }

    #[test]
    fn it_rejects_a_last_modified_older_than_the_file() {
        let modified = UNIX_EPOCH + Duration::from_nanos(1_700_000_000_721_955_581);
        let stale = modified - Duration::from_secs(1);

        let mut headers = HeaderMap::new();
        headers.insert(
            IF_MODIFIED_SINCE,
            HeaderValue::from_str(&httpdate::fmt_http_date(stale)).unwrap(),
        );

        assert!(!validate_last_modified(
            modified,
            &HttpHeaders::from(headers)
        ));
    }

    #[test]
    fn it_validates_etag_list() {
        let mut headers = HeaderMap::new();
        headers.insert(
            IF_NONE_MATCH,
            HeaderValue::from_static("\"123\",\"321\",\"111\""),
        );

        let headers = HttpHeaders::from(headers);

        assert!(validate_etag(&ETag::strong("123"), &headers));
    }

    #[test]
    fn it_validates_etag_not_present_in_list() {
        let mut headers = HeaderMap::new();
        headers.insert(
            IF_NONE_MATCH,
            HeaderValue::from_static("\"123\",\"321\",\"111\""),
        );

        let headers = HttpHeaders::from(headers);

        assert!(!validate_etag(&ETag::strong("555"), &headers));
    }

    #[test]
    fn it_validates_etag_single() {
        let mut headers = HeaderMap::new();
        headers.insert(IF_NONE_MATCH, HeaderValue::from_static("\"123\""));

        let headers = HttpHeaders::from(headers);

        assert!(validate_etag(&ETag::strong("123"), &headers));
    }

    #[test]
    fn it_validates_etag_single_different_value() {
        let mut headers = HeaderMap::new();
        headers.insert(IF_NONE_MATCH, HeaderValue::from_static("\"123\""));

        let headers = HttpHeaders::from(headers);

        assert!(!validate_etag(&ETag::strong("555"), &headers));
    }

    #[test]
    fn it_validates_etag_when_if_none_match_missing() {
        let headers = HttpHeaders::from(HeaderMap::new());

        assert!(!validate_etag(&ETag::strong("123"), &headers));
    }

    #[test]
    fn it_validates_last_modified() {
        let now = SystemTime::now();
        let mut headers = HeaderMap::new();
        headers.insert(
            IF_MODIFIED_SINCE,
            HeaderValue::from_str(&httpdate::fmt_http_date(now)).unwrap(),
        );

        let headers = HttpHeaders::from(headers);

        assert!(validate_last_modified(
            now - Duration::from_secs(10),
            &headers
        ));
    }

    #[test]
    fn it_validates_last_modified_resource_has_been_updated() {
        let now = SystemTime::now();
        let mut headers = HeaderMap::new();
        headers.insert(
            IF_MODIFIED_SINCE,
            HeaderValue::from_str(&httpdate::fmt_http_date(now)).unwrap(),
        );

        let headers = HttpHeaders::from(headers);

        assert!(!validate_last_modified(
            now + Duration::from_secs(10),
            &headers
        ));
    }

    #[test]
    fn it_validates_last_modified_when_if_modified_since_missing() {
        let now = SystemTime::now();
        let headers = HttpHeaders::from(HeaderMap::new());

        assert!(!validate_last_modified(now, &headers));
    }
}
