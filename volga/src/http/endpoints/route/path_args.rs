//! URL path arguments utilities

use smallvec::SmallVec;
use super::DEFAULT_DEPTH;
use crate::error::Error;

const QUERY_SEPARATOR: char = '&';
const QUERY_KEY_VALUE_SEPARATOR: char = '=';
const DEFAULT_PARAM_SIZE: usize = 6;

/// Route path arguments
pub(crate) type PathArgs = SmallVec<[PathArg; DEFAULT_DEPTH]>;

/// A single path argument
#[derive(Clone)]
pub(crate) struct PathArg {
    /// Argument name
    pub(crate) name: Box<str>,

    /// Argument value
    pub(crate) value: Box<str>,
}

impl PathArg {
    /// Creates an empty, zero-cost iterator of route path arguments
    #[inline(always)]
    pub(crate) fn empty<const N: usize>() -> smallvec::IntoIter<[PathArg; N]> {
        SmallVec::<[PathArg; N]>::new().into_iter()
    }
    
    #[inline]
    pub(crate) fn make_query_str(args: &PathArgs) -> Result<String, Error> {
        use std::fmt::Write;
        
        if args.is_empty() { 
            return Err(Error::client_error("Path parsing error: missing arguments"));
        } 
        
        let mut result = String::with_capacity(args.len() * DEFAULT_PARAM_SIZE);
        let mut iter = args.iter();
        if let Some(first) = iter.next() {
            write!(result, "{}{QUERY_KEY_VALUE_SEPARATOR}{}", 
                   first.name, 
                   first.value)
                .map_err(Error::from)?;
            for s in iter {
                write!(result, "{QUERY_SEPARATOR}{}{QUERY_KEY_VALUE_SEPARATOR}{}",
                       s.name,
                       s.value)
                    .map_err(Error::from)?;
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn it_makes_query_str() {
        let args: PathArgs = smallvec::smallvec![
            PathArg { name: "id".into(), value: "123".into() },
            PathArg { name: "name".into(), value: "John".into() }
        ];
        
        let query_str = PathArg::make_query_str(&args).unwrap();
        assert_eq!(query_str, "id=123&name=John");
    }

    #[test]
    fn it_makes_query_str_empty() {
        let args: PathArgs = smallvec::smallvec![];
        
        let result = PathArg::make_query_str(&args);
        assert!(result.is_err());
    }
    
    #[test]
    fn it_creates_empty_path_args_iter() {
        let mut iter = PathArg::empty::<DEFAULT_DEPTH>();
        let item = iter.next();
        assert!(item.is_none());
    }
}