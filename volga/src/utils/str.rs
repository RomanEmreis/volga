//! Utilities for `String`, `str`, `[u8]`

use memchr::memchr_iter;

/// Search for the first occurrence of a byte in a slice using [`memchr::memchr`]
#[inline(always)]
pub fn memchr_contains(needle: u8, haystack: &[u8]) -> bool {
    memchr::memchr(needle, haystack).is_some()
}

/// Splits a byte slice by a delimiter using [`memchr::memchr_iter`]
#[inline]
pub(crate) fn memchr_split(delimiter: u8, value: &[u8]) -> MemchrSplitIter<'_> {
    MemchrSplitIter {
        value,
        iter: memchr_iter(delimiter, value),
        last: 0,
    }
}

/// Splits a string slice by an ASCII delimiter using [`memchr::memchr_iter`], excluding
/// empty substrings.
///
/// The substrings are sliced out of `value` at the positions the delimiter was found at,
/// which are always character boundaries for an ASCII byte - so, unlike splitting the bytes
/// and reading each part back as UTF-8, nothing is validated twice.
#[inline]
pub(crate) fn memchr_split_nonempty(delimiter: u8, value: &str) -> MemchrSplitNonEmpty<'_> {
    debug_assert!(
        delimiter.is_ascii(),
        "the delimiter has to be an ASCII byte"
    );

    MemchrSplitNonEmpty {
        value,
        iter: memchr_iter(delimiter, value.as_bytes()),
        last: 0,
    }
}

/// An iterator over the substrings of a byte slice separated by a delimiter
pub(crate) struct MemchrSplitIter<'a> {
    value: &'a [u8],
    iter: memchr::Memchr<'a>,
    last: usize,
}

/// An iterator over the substrings of a string slice separated by an ASCII delimiter,
/// excluding empty substrings.
#[derive(Clone)]
pub(crate) struct MemchrSplitNonEmpty<'a> {
    value: &'a str,
    iter: memchr::Memchr<'a>,
    last: usize,
}

impl<'a> Iterator for MemchrSplitNonEmpty<'a> {
    type Item = &'a str;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        for pos in self.iter.by_ref() {
            let start = self.last;
            let end = pos;
            self.last = pos + 1;

            if end > start {
                return Some(&self.value[start..end]);
            }
        }

        let start = self.last;
        if start < self.value.len() {
            self.last = self.value.len();
            return Some(&self.value[start..]);
        }

        None
    }
}

impl<'a> Iterator for MemchrSplitIter<'a> {
    type Item = &'a [u8];

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        if let Some(pos) = self.iter.next() {
            let start = self.last;
            self.last = pos + 1;
            Some(&self.value[start..pos])
        } else if self.last > self.value.len() {
            None
        } else {
            let tail = &self.value[self.last..];
            self.last = self.value.len() + 1;
            Some(tail)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn it_splits_str() {
        let str = "asdsa,faf,dfd,dfffffff,fdfsdfdsfd,";

        let parts = memchr_split(b',', str.as_bytes()).collect::<Vec<_>>();

        assert_eq!(parts.len(), 6);

        assert_eq!(parts[0], b"asdsa");
        assert_eq!(parts[1], b"faf");
        assert_eq!(parts[2], b"dfd");
        assert_eq!(parts[3], b"dfffffff");
        assert_eq!(parts[4], b"fdfsdfdsfd");
        assert_eq!(parts[5], b"");
    }

    #[test]
    fn it_splits_non_empty_str() {
        let str = "asdsa,faf,dfd,,dfffffff,fdfsdfdsfd,";

        let parts = memchr_split_nonempty(b',', str).collect::<Vec<_>>();

        assert_eq!(parts, ["asdsa", "faf", "dfd", "dfffffff", "fdfsdfdsfd"]);
    }

    #[test]
    fn it_splits_empty_str() {
        let str = "";

        let parts = memchr_split(b',', str.as_bytes()).collect::<Vec<_>>();

        assert_eq!(parts.len(), 1);
        assert_eq!(parts[0], b"");
    }

    #[test]
    fn it_splits_non_empty_empty_str() {
        let str = "";

        let parts = memchr_split_nonempty(b',', str).collect::<Vec<_>>();

        assert_eq!(parts.len(), 0);
    }

    #[test]
    fn it_splits_non_empty_str_around_multibyte_characters() {
        let str = "/\u{447}\u{430}\u{441}\u{442}\u{44c}//\u{1f980}/x/";

        let parts = memchr_split_nonempty(b'/', str).collect::<Vec<_>>();

        assert_eq!(
            parts,
            ["\u{447}\u{430}\u{441}\u{442}\u{44c}", "\u{1f980}", "x"]
        );
    }
}
