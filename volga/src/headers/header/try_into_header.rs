//! A conversion tools for HTTP headers of different types

use super::{FromHeaders, Header, HeaderName, HeaderValue};
use crate::error::Error;

/// A trait that describes how to convert a type 
/// into a raw HTTP [`HeaderName`] and [`HeaderValue`] pair.
pub trait TryIntoHeaderPair {
    /// Converts into a raw HTTP [`HeaderName`] and [`HeaderValue`] pair.
    fn try_into_pair(self) -> Result<(HeaderName, HeaderValue), Error>;
}

impl<T: FromHeaders> TryIntoHeaderPair for Header<T> {
    #[inline]
    fn try_into_pair(self) -> Result<(HeaderName, HeaderValue), Error> {
        Ok((T::NAME, self.into_inner()))
    }
}

impl<K, V> TryIntoHeaderPair for (K, V)
where
    HeaderName: TryFrom<K>,
    HeaderValue: TryFrom<V>,
    Error: From<<HeaderName as TryFrom<K>>::Error>,
    Error: From<<HeaderValue as TryFrom<V>>::Error>,
{
    #[inline]
    fn try_into_pair(self) -> Result<(HeaderName, HeaderValue), Error> {
        let (k, v) = self;
        let name = HeaderName::try_from(k).map_err(Error::from)?;
        let value = HeaderValue::try_from(v).map_err(Error::from)?;
        Ok((name, value))
    }
}

#[cfg(test)]
mod tests {
    use crate::headers::ContentType;
    use super::*;

    #[test]
    fn it_converts_from_header() {
        let header = ContentType::from_static("text/plain");
        let (name, value) = header.try_into_pair().unwrap();

        assert_eq!(name, "content-type");
        assert_eq!(value, "text/plain");
    }

    #[test]
    fn it_converts_from_tuple() {
        let header_tuple = ("content-type", "text/plain");
        let (name, value) = header_tuple.try_into_pair().unwrap();

        assert_eq!(name, "content-type");
        assert_eq!(value, "text/plain");
    }
}