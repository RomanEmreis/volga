use std::time::SystemTime;
use httpdate::parse_http_date;

use crate::headers::{
    ETag, 
    Headers, 
    IF_MODIFIED_SINCE, 
    IF_NONE_MATCH
};

#[inline]
#[allow(dead_code)]
pub(crate) fn validate_etag(etag: &ETag, headers: &Headers) -> bool {
    headers.get(&IF_NONE_MATCH)
        .map(|if_none_match| *if_none_match == etag.as_ref())
        .unwrap_or(false)
}

#[inline]
#[allow(dead_code)]
pub(crate) fn validate_last_modified(last_modified: SystemTime, headers: &Headers) -> bool {
    let last_modified = Some(last_modified);
    headers.get(&IF_MODIFIED_SINCE)
        .map(|if_modified_since| last_modified == if_modified_since.to_str().ok().and_then(|s| parse_http_date(s).ok()))
        .unwrap_or(false)
}