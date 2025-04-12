//! Utilities for `String`, `str`, `[u8]`

use memchr::memchr;

pub(crate) fn memchr_split(delimiter: u8, value: &[u8]) -> MemchrSplit<'_> {
    MemchrSplit {
        delimiter,
        value: Some(value),
    }
}

pub(crate) struct MemchrSplit<'a> {
    delimiter: u8,
    value: Option<&'a [u8]>,
}

impl<'a> Iterator for MemchrSplit<'a> {
    type Item = &'a [u8];
    fn next(&mut self) -> Option<Self::Item> {
        let value = self.value?;
        if let Some(pos) = memchr(self.delimiter, value) {
            let (front, back) = value.split_at(pos);
            self.value = Some(&back[1..]);
            Some(front)
        } else {
            self.value.take()
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
}