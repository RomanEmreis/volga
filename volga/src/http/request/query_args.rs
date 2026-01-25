//! Query string parsing utilities for `HttpRequest::query_args()`.
//!
//! This module provides a low-allocation way to iterate over URL query parameters.
//! The design is optimized for repeated access across multiple middleware/extractors:
//!
//! - The first call builds a cache of byte spans (`QueryArgsCache`) for `key=value` pairs.
//! - Subsequent calls reuse the cache and only slice the original query string.
//!
//! ## Semantics
//! - Only `key=value` pairs are returned.
//! - Segments without `=` are ignored (e.g. `flag` is skipped).
//! - If a segment contains multiple `=`, only the first `=` splits key and value
//!   (e.g. `a=1=2` => key=`a`, value=`1=2`).
//! - No percent-decoding is performed. Returned `&str` are raw slices of the original query.
//!
//! ## Safety / UTF-8
//! Spans are byte offsets into the original query string. Offsets are created only at ASCII
//! delimiters (`&` and `=`), which are always valid UTF-8 boundaries, so slicing is safe.

const DEFAULT_PARAMS_COUNT: usize = 4;
const KV_SEPARATOR: u8 = b'=';
const PARAM_SEPARATOR: u8 = b'&';

pub(super) struct QueryArgsIter<'a> {
    query: &'a str,
    iter: std::slice::Iter<'a, QueryArgSpan>,
}

impl<'a> QueryArgsIter<'a> {
    /// Creates an iterator over cached query param spans.
    ///
    /// `cache` must be built from the same `query` string content.
    #[inline]
    pub(super) fn new(query: &'a str, cache: &'a QueryArgsCache) -> Self {
        // Future-proof: if someone ever changes how URI/query is stored, this catches
        // accidental reuse of a cache with a different query string.
        debug_assert_eq!(cache.query_len, query.len());

        Self {
            query,
            iter: cache.pairs.iter(),
        }
    }
}

impl<'a> Iterator for QueryArgsIter<'a> {
    type Item = (&'a str, &'a str);

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        self.iter.next().map(|s| {
            (
                &self.query[s.key_start..s.key_end],
                &self.query[s.value_start..s.value_end],
            )
        })
    }
}

#[derive(Debug, Clone, Copy)]
struct QueryArgSpan {
    key_start: usize,
    key_end: usize,
    value_start: usize,
    value_end: usize,
}

/// Cache of parsed `key=value` spans for a query string.
///
/// This cache stores byte offsets only (no decoded values), so it is cheap to build and
/// cheap to reuse. It is intended to be stored on the request and reused by multiple
/// middleware/extractors.
pub(super) struct QueryArgsCache {
    query_len: usize,
    pairs: smallvec::SmallVec<[QueryArgSpan; DEFAULT_PARAMS_COUNT]>,
}

impl QueryArgsCache {
    /// Parses query string into spans of `key=value` pairs.
    ///
    /// Notes:
    /// - Only segments containing `=` are stored.
    /// - Segments are separated by `&`.
    /// - No percent-decoding is performed.
    pub(super) fn new(query: &str) -> Self {
        let mut pairs = smallvec::SmallVec::new();

        // Single pass over bytes: identify segments split by '&', and first '=' within each segment.
        let bytes = query.as_bytes();
        let mut seg_start = 0usize;
        let mut eq_pos: Option<usize> = None;

        // Iterate indices 0..=len to flush last segment at len.
        for i in 0..=bytes.len() {
            let is_end = i == bytes.len();
            let b = if is_end { PARAM_SEPARATOR } else { bytes[i] };

            if !is_end && b == KV_SEPARATOR && eq_pos.is_none() {
                eq_pos = Some(i);
                continue;
            }

            if b == b'&' {
                if let Some(eq) = eq_pos {
                    // Segment is [seg_start..i], '=' at eq.
                    // key:   [seg_start..eq]
                    // value: [eq+1..i]
                    pairs.push(QueryArgSpan {
                        key_start: seg_start,
                        key_end: eq,
                        value_start: eq + 1,
                        value_end: i,
                    });
                }

                // Next segment starts after '&'
                seg_start = i + 1;
                eq_pos = None;
            }
        }

        Self {
            query_len: query.len(),
            pairs,
        }
    }

    #[cfg(test)]
    fn collect<'a>(&'a self, query: &'a str) -> Vec<(&'a str, &'a str)> {
        QueryArgsIter::new(query, self).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(query: &str) -> Vec<(String, String)> {
        let cache = QueryArgsCache::new(query);
        QueryArgsIter::new(query, &cache)
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect()
    }

    #[test]
    fn it_parses_basic_pairs() {
        assert_eq!(parse("a=1&b=2"), vec![("a".into(), "1".into()), ("b".into(), "2".into())]);
    }

    #[test]
    fn it_ignores_segments_without_equals() {
        // "flag" and "x" are ignored
        assert_eq!(parse("flag&a=1&x&b=2"), vec![("a".into(), "1".into()), ("b".into(), "2".into())]);
    }

    #[test]
    fn it_handles_empty_query() {
        assert_eq!(parse(""), Vec::<(String, String)>::new());
    }

    #[test]
    fn it_handles_trailing_ampersand_and_empty_segments() {
        assert_eq!(parse("a=1&"), vec![("a".into(), "1".into())]);
        assert_eq!(parse("a=1&&b=2"), vec![("a".into(), "1".into()), ("b".into(), "2".into())]);
        assert_eq!(parse("&&"), Vec::<(String, String)>::new());
    }

    #[test]
    fn it_allows_empty_key_or_value() {
        assert_eq!(parse("=1&a="), vec![("".into(), "1".into()), ("a".into(), "".into())]);
    }

    #[test]
    fn it_uses_first_equals_in_segment() {
        assert_eq!(parse("a=1=2&b==3"), vec![("a".into(), "1=2".into()), ("b".into(), "=3".into())]);
    }

    #[test]
    fn it_supports_utf8_content() {
        // UTF-8 is preserved as-is (no decoding).
        assert_eq!(parse("name=Roman&city=London"), vec![("name".into(), "Roman".into()), ("city".into(), "London".into())]);
    }

    #[test]
    fn cache_len_matches_query_len() {
        let q = "a=1&b=2";
        let cache = QueryArgsCache::new(q);
        assert_eq!(cache.query_len, q.len());
    }

    #[test]
    fn spans_are_consistent_and_sliceable() {
        let q = "a=1&bb=22&ccc=333";
        let cache = QueryArgsCache::new(q);

        // Ensure each span is within bounds and slices are correct
        for (k, v) in cache.collect(q) {
            assert!(!k.contains('&'));
            assert!(!k.contains('='));
            // values may contain '=' in general (if present after first '='), but not '&'
            assert!(!v.contains('&'));
        }

        assert_eq!(cache.collect(q), vec![("a", "1"), ("bb", "22"), ("ccc", "333")]);
    }
}