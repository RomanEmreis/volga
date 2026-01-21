//! URL path arguments utilities

use smallvec::SmallVec;
use super::DEFAULT_DEPTH;
use crate::error::Error;
use std::sync::Arc;
use std::sync::OnceLock;

const QUERY_SEPARATOR: char = '&';
const QUERY_KEY_VALUE_SEPARATOR: char = '=';

/// Route path arguments
#[derive(Debug)]
pub struct PathArgs {
    args: SmallVec<[PathArg; DEFAULT_DEPTH]>,
    query_str: OnceLock<String>,
}

/// A single matched path argument.
///
/// This type is part of Volga's public API but is primarily intended
/// for framework-level extractors and middleware. It should not be
/// constructed manually.
#[derive(Debug, Clone)]
pub struct PathArg {
    /// Argument name
    pub(crate) name: Arc<str>,

    /// Argument value
    pub(crate) value: Arc<str>,
}

impl PathArgs {
    #[inline]
    pub(crate) fn new() -> Self {
        Self {
            args: SmallVec::new(),
            query_str: OnceLock::new(),
        }
    }

    /// Returns an iterator over the args.
    ///
    /// The iterator yields all items from start to end.
    #[inline]
    pub(crate) fn iter(&self) -> std::slice::Iter<'_, PathArg> {
        self.args.iter()
    }

    /// Returns the first arg, or `None` if it is empty.
    #[inline]
    #[allow(unused)]
    pub(crate) fn first(&self) -> Option<&PathArg> {
        self.args.first()
    }

    /// Append an item to the args vector.
    #[inline]
    pub(crate) fn push(&mut self, arg: PathArg) {
        self.args.push(arg);
        let _ = self.query_str.take();
    }

    /// Restures a query string of this route
    #[inline]
    pub(crate) fn query_str(&self) -> Result<&str, Error> {
        if self.args.is_empty() {
            return Err(Error::client_error("Path parsing error: missing arguments"));
        }

        let value = self
            .query_str
            .get_or_init(|| make_query_string(&self.args));

        Ok(value.as_str())
    }

    /// Splits [`PathArgs`] into parts
    #[inline]
    pub(crate) fn into_parts(self) -> (SmallVec<[PathArg; DEFAULT_DEPTH]>, Option<String>) {
        let cached = self.query_str.into_inner();
        (self.args, cached)
    }

    /// Creates [`PathArgs`] from parts
    #[inline]
    pub(crate) fn from_parts(
        args: SmallVec<[PathArg; DEFAULT_DEPTH]>,
        cached: Option<String>,
    ) -> Self {
        let query_str = OnceLock::new();
        if let Some(value) = cached {
            let _ = query_str.set(value);
        }
        Self { args, query_str }
    }
}

impl Default for PathArgs {
    #[inline]
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for PathArgs {
    #[inline]
    fn clone(&self) -> Self {
        Self {
            args: self.args.clone(),
            query_str: OnceLock::new(),
        }
    }
}

impl FromIterator<PathArg> for PathArgs {
    #[inline]
    fn from_iter<T: IntoIterator<Item = PathArg>>(iter: T) -> Self {
        let mut args = PathArgs::new();
        for arg in iter {
            args.args.push(arg);
        }
        args
    }
}

impl From<SmallVec<[PathArg; DEFAULT_DEPTH]>> for PathArgs {
    #[inline]
    fn from(args: SmallVec<[PathArg; DEFAULT_DEPTH]>) -> Self {
        Self {
            args,
            query_str: OnceLock::new(),
        }
    }
}

impl IntoIterator for PathArgs {
    type Item = PathArg;
    type IntoIter = smallvec::IntoIter<[PathArg; DEFAULT_DEPTH]>;

    #[inline]
    fn into_iter(self) -> Self::IntoIter {
        self.args.into_iter()
    }
}

#[inline]
fn make_query_string(args: &SmallVec<[PathArg; DEFAULT_DEPTH]>) -> String  {
    let capacity = args.iter().fold(0, |acc, arg| {
        acc + arg.name.len() + arg.value.len() + 1
    }) + args.len().saturating_sub(1);

    let mut result = String::with_capacity(capacity);
    let mut iter = args.iter();

    if let Some(first) = iter.next() {
        result.push_str(first.name.as_ref());
        result.push(QUERY_KEY_VALUE_SEPARATOR);
        result.push_str(first.value.as_ref());
        for s in iter {
            result.push(QUERY_SEPARATOR);
            result.push_str(s.name.as_ref());
            result.push(QUERY_KEY_VALUE_SEPARATOR);
            result.push_str(s.value.as_ref());
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn it_makes_query_str() {
        let args: PathArgs = smallvec::smallvec![
            PathArg { name: "id".into(), value: "123".into() },
            PathArg { name: "name".into(), value: "John".into() }
        ].into();
        
        let query_str = args.query_str().unwrap();
        assert_eq!(query_str, "id=123&name=John");
    }

    #[test]
    fn it_makes_query_str_empty() {
        let args: PathArgs = smallvec::smallvec![].into();
        
        let result = args.query_str();
        assert!(result.is_err());
    }
}